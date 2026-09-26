// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Running `exec` and `serve` requests: acquiring a pane, then polling each
//! in-flight run, watch and notify request until it can be answered.

use crate::capture::{RunRequest, WatchRequest};
use std::io::Write;
use std::time::{Duration, Instant};

const TAIL_SUCCESS: usize = 30;
const TAIL_FAILURE: usize = 200;

fn read_pane_tail(
    session: &crate::session::Session,
    pane_id: u32,
    max_lines: usize,
) -> (usize, String) {
    session
        .read_pane(pane_id, max_lines)
        .unwrap_or((0, String::new()))
}

fn tail_hint(total_lines: usize, shown: usize, pane_id: u32) -> Option<String> {
    if shown < total_lines {
        Some(format!(
            "showing last {shown} of {total_lines} lines; read {pane_id} {total_lines} returns more until this pane is reused"
        ))
    } else {
        None
    }
}

/// Start an `exec` in a pane; `None` when answered already or queued as notify.
pub fn start_run(
    cmd: &str,
    timeout_secs: u32,
    no_timeout: bool,
    notify_pattern: Option<String>,
    session: &mut crate::session::Session,
    mut stream: std::os::unix::net::UnixStream,
    srv: &mut crate::server::Server,
) -> Option<crate::capture::RunRequest> {
    let pane_id = match session.acquire_exec_pane(cmd) {
        Ok(id) => id,
        Err(e) => {
            let _ = stream.write_all(format!("{{\"error\":\"{e}\"}}\n").as_bytes());
            return None;
        }
    };

    let deadline = if no_timeout {
        None
    } else {
        Some(Instant::now() + Duration::from_secs(timeout_secs as u64))
    };

    if let Some(pattern) = notify_pattern {
        let json = serde_json::json!({
            "pane_id": pane_id,
            "started": true,
            "pid": session.pane_by_id(pane_id).and_then(|p| p.pty_opt().map(|pty| pty.child_pid.as_raw())).unwrap_or(0),
        });
        let _ = stream.write_all(
            format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
        );

        srv.notify_requests.push(crate::server::NotifyRequest {
            task_id: String::new(),
            pane_id,
            notify_pattern: pattern,
            deadline,
            kill_on_timeout: true,
            draining: false,
            exit_status: None,
        });
        return None;
    }

    Some(RunRequest {
        stream,
        pane_id,
        deadline,
        kill_on_timeout: true,
        exit_status: None,
        draining: false,
    })
}

/// What a `serve` with a ready pattern should run and wait for.
pub struct WatchSpec<'a> {
    pub cmd: &'a str,
    pub pattern: &'a str,
    pub timeout_secs: u32,
    pub no_timeout: bool,
}

/// Start a `serve` that waits for a ready pattern; `None` when answered already.
pub fn start_watch(
    spec: WatchSpec<'_>,
    session: &mut crate::session::Session,
    mut stream: std::os::unix::net::UnixStream,
) -> Option<WatchRequest> {
    let pane_id = match session.acquire_serve_pane(spec.cmd) {
        Ok(id) => id,
        Err(e) => {
            let _ = stream.write_all(format!("{{\"error\":\"{e}\"}}\n").as_bytes());
            return None;
        }
    };

    let deadline = if spec.no_timeout {
        None
    } else {
        Some(Instant::now() + Duration::from_secs(spec.timeout_secs as u64))
    };

    Some(WatchRequest {
        stream,
        pane_id,
        pattern: spec.pattern.to_string(),
        deadline,
        draining: false,
    })
}

/// Start a `serve` with no ready pattern and answer at once.
pub fn start_serve_no_pattern(
    cmd: &str,
    session: &mut crate::session::Session,
    mut stream: std::os::unix::net::UnixStream,
) {
    let pane_id = match session.acquire_serve_pane(cmd) {
        Ok(id) => id,
        Err(e) => {
            let _ = stream.write_all(format!("{{\"error\":\"{e}\"}}\n").as_bytes());
            return;
        }
    };

    let pid = session
        .pane_by_id(pane_id)
        .and_then(|p| p.pty_opt().map(|pty| pty.child_pid.as_raw()))
        .unwrap_or(0);
    let json = serde_json::json!({
        "pid": pid,
        "pane_id": pane_id,
        "started": true,
        "alive": true,
        "ready": serde_json::Value::Null,
        "timeout": false,
        "matched": serde_json::Value::Null,
    });
    let _ = stream
        .write_all(format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes());
}

/// Answer each `exec` whose child has exited and drained, or timed out.
pub fn check_run_requests(requests: &mut Vec<RunRequest>, session: &mut crate::session::Session) {
    let mut i = 0;
    while i < requests.len() {
        let pane_id = requests[i].pane_id;

        if let Some(pane) = session.pane_by_id_mut(pane_id) {
            let _ = pane.feed_output();
        }

        if session.pane_by_id(pane_id).is_none() {
            let mut req = requests.remove(i);
            let json = serde_json::json!({
                "error": "pane killed externally",
                "pane_id": pane_id,
            });
            let _ = req.stream.write_all(
                format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
            );
            continue;
        }

        if requests[i].draining {
            let still_has_data = session
                .pane_by_id_mut(pane_id)
                .map(|p| p.feed_output().unwrap_or(false))
                .unwrap_or(false);
            if !still_has_data {
                let exit_code = requests[i].exit_status.unwrap_or(1);
                let mut req = requests.remove(i);
                let tail_n = if exit_code == 0 {
                    TAIL_SUCCESS
                } else {
                    TAIL_FAILURE
                };
                let (total_lines, output) = read_pane_tail(session, pane_id, tail_n);
                session.mark_pane_completed(pane_id, exit_code);
                session.needs_redraw = true;
                let shown = output.lines().count();
                let hint = tail_hint(total_lines, shown, pane_id);
                let json = serde_json::json!({
                    "exit_code": exit_code,
                    "pane_id": pane_id,
                    "output": output,
                    "timeout": false,
                    "total_lines": total_lines,
                    "tail_shown": shown,
                    "hint": hint,
                });
                let _ = req.stream.write_all(
                    format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
                );
                continue;
            }
            i += 1;
            continue;
        }

        if let Some(pane) = session.pane_by_id_mut(pane_id)
            && let Some(pty) = pane.pty_opt_mut()
            && let Some(code) = pty.poll_exit()
        {
            requests[i].exit_status = Some(code);
            requests[i].draining = true;
            continue;
        }

        if let Some(deadline) = requests[i].deadline
            && Instant::now() >= deadline
        {
            let mut req = requests.remove(i);
            let (total_lines, output) = read_pane_tail(session, pane_id, TAIL_FAILURE);
            if req.kill_on_timeout
                && let Some(pane) = session.pane_by_id(pane_id)
            {
                pane.kill_process_tree();
            }
            session.mark_pane_completed(pane_id, -1);
            session.needs_redraw = true;
            let shown = output.lines().count();
            let hint = tail_hint(total_lines, shown, pane_id);
            let json = serde_json::json!({
                "exit_code": null,
                "pane_id": pane_id,
                "output": output,
                "timeout": true,
                "total_lines": total_lines,
                "tail_shown": shown,
                "hint": hint,
            });
            let _ = req.stream.write_all(
                format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
            );
            continue;
        }

        i += 1;
    }
}

/// Answer each `serve` whose pattern matched, child exited, or deadline passed.
pub fn check_watch_requests(
    requests: &mut Vec<WatchRequest>,
    session: &mut crate::session::Session,
) {
    let mut i = 0;
    while i < requests.len() {
        let pane_id = requests[i].pane_id;

        if let Some(pane) = session.pane_by_id_mut(pane_id) {
            let _ = pane.feed_output();
        }

        if session.pane_by_id(pane_id).is_none() {
            let mut req = requests.remove(i);
            let json = serde_json::json!({
                "error": "pane killed externally",
                "pane_id": pane_id,
            });
            let _ = req.stream.write_all(
                format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
            );
            continue;
        }

        if requests[i].draining {
            let still_has_data = session
                .pane_by_id_mut(pane_id)
                .map(|p| p.feed_output().unwrap_or(false))
                .unwrap_or(false);
            if !still_has_data {
                let mut req = requests.remove(i);
                let pid = session
                    .pane_by_id(pane_id)
                    .and_then(|p| p.pty_opt().map(|pty| pty.child_pid.as_raw()))
                    .unwrap_or(0);
                let json = serde_json::json!({
                    "pid": pid,
                    "pane_id": pane_id,
                    "started": true,
                    "alive": false,
                    "ready": false,
                    "timeout": false,
                    "matched": serde_json::Value::Null,
                });
                let _ = req.stream.write_all(
                    format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
                );
                continue;
            }
            i += 1;
            continue;
        }

        if let Some(matched) = session.check_pattern(pane_id, &requests[i].pattern) {
            let mut req = requests.remove(i);
            let pid = session
                .pane_by_id(pane_id)
                .and_then(|p| p.pty_opt().map(|pty| pty.child_pid.as_raw()))
                .unwrap_or(0);
            let json = serde_json::json!({
                "pid": pid,
                "pane_id": pane_id,
                "started": true,
                "alive": true,
                "ready": true,
                "timeout": false,
                "matched": matched,
            });
            let _ = req.stream.write_all(
                format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
            );
            continue;
        }

        if let Some(pane) = session.pane_by_id_mut(pane_id)
            && let Some(pty) = pane.pty_opt_mut()
            && pty.poll_exit().is_some()
        {
            requests[i].draining = true;
            continue;
        }

        if let Some(deadline) = requests[i].deadline
            && Instant::now() >= deadline
        {
            let mut req = requests.remove(i);
            let pid = session
                .pane_by_id(pane_id)
                .and_then(|p| p.pty_opt().map(|pty| pty.child_pid.as_raw()))
                .unwrap_or(0);
            let json = serde_json::json!({
                "pid": pid,
                "pane_id": pane_id,
                "started": true,
                "alive": true,
                "ready": false,
                "timeout": true,
                "matched": serde_json::Value::Null,
            });
            let _ = req.stream.write_all(
                format!("{}\n", serde_json::to_string(&json).unwrap_or_default()).as_bytes(),
            );
            continue;
        }

        i += 1;
    }
}

#[cfg(test)]
#[path = "tests/native_exec/mod.rs"]
mod tests;

/// Post notify events for pattern, exit or timeout to every service inbox.
pub fn check_notify_requests(
    requests: &mut Vec<crate::server::NotifyRequest>,
    session: &mut crate::session::Session,
) {
    let mut i = 0;
    while i < requests.len() {
        let pane_id = requests[i].pane_id;
        if let Some(pane) = session.pane_by_id_mut(pane_id) {
            let _ = pane.feed_output();
        }

        if session.pane_by_id(pane_id).is_none() {
            requests.remove(i);
            continue;
        }

        let mut remove = false;
        let mut payload = None;

        let mut matched_now = false;
        if !requests[i].notify_pattern.is_empty() {
            let pattern_str = requests[i].notify_pattern.clone();
            if let Some(matched) = session.check_pattern(pane_id, &pattern_str) {
                requests[i].notify_pattern.clear();
                matched_now = true;
                payload = Some(serde_json::json!({
                    "event_type": "exec_notify",
                    "pane_id": pane_id,
                    "trigger_reason": "pattern_match",
                    "payload": { "stdout_tail": matched }
                }));
            }
        }

        if matched_now {
            // why: the request stays queued so the later process exit still notifies.
        } else if let Some(pane) = session.pane_by_id_mut(pane_id) {
            if let Some(pty) = pane.pty_opt_mut() {
                if let Some(exit_status) = pty.poll_exit() {
                    remove = true;
                    payload = Some(serde_json::json!({
                        "event_type": "exec_notify",
                        "pane_id": pane_id,
                        "trigger_reason": "process_exit",
                        "payload": { "exit_status": exit_status }
                    }));
                } else if !crate::capture::pid_exists(pty.child_pid.as_raw()) {
                    remove = true;
                    payload = Some(serde_json::json!({
                        "event_type": "exec_notify",
                        "pane_id": pane_id,
                        "trigger_reason": "process_exit",
                        "payload": { "exit_status": 0 }
                    }));
                } else if !requests[i].draining {
                    if let Some(deadline) = requests[i].deadline {
                        if Instant::now() >= deadline {
                            requests[i].draining = true;
                            let _ = unsafe { libc::kill(pty.child_pid.as_raw(), libc::SIGTERM) };
                            payload = Some(serde_json::json!({
                                "event_type": "exec_notify",
                                "pane_id": pane_id,
                                "trigger_reason": "timeout",
                                "payload": {}
                            }));
                        }
                    }
                }
            }
        }

        if let Some(p) = payload {
            let msg = serde_json::to_string(&p).unwrap_or_default();
            for pane in session.panes.iter() {
                if let crate::pane::PaneKind::Service { inner, .. } = &pane.kind {
                    if let Some(sender) = inner.ipc_sender() {
                        sender.from_verb(msg.clone());
                    }
                }
            }
        }

        if remove {
            requests.remove(i);
        } else {
            i += 1;
        }
    }
}
