// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Executing decoded requests: each queued action runs against the session
//! and either answers its stream now or is parked until it can.

use crate::capture::ApiAction;
use crate::server;
use crate::session::Session;
use std::io::Write;

/// Run every action the server has queued since the last tick.
pub fn handle_api_actions(srv: &mut server::Server, session: &mut Session) -> std::io::Result<()> {
    for action in srv.drain_actions() {
        match action {
            ApiAction::Response(msg, mut stream) => {
                let _ = stream.write_all(msg.as_bytes());
            }
            ApiAction::Exec {
                cmd,
                timeout_secs,
                no_timeout,
                notify_pattern,
                stream,
            } => {
                if let Some(req) = crate::commands::start_run(
                    &cmd,
                    timeout_secs,
                    no_timeout,
                    notify_pattern,
                    session,
                    stream,
                    srv,
                ) {
                    srv.run_requests.push(req);
                }
            }
            ApiAction::Serve {
                cmd,
                pattern,
                timeout_secs,
                no_timeout,
                stream,
            } => match pattern {
                Some(pat) => {
                    if let Some(req) = crate::commands::start_watch(
                        crate::commands::WatchSpec {
                            cmd: &cmd,
                            pattern: &pat,
                            timeout_secs,
                            no_timeout,
                        },
                        session,
                        stream,
                    ) {
                        srv.watch_requests.push(req);
                    }
                }
                None => {
                    crate::commands::start_serve_no_pattern(&cmd, session, stream);
                }
            },
            ApiAction::Send {
                pane_id,
                data,
                mut stream,
            } => {
                if server::check_sid_protection(&stream, pane_id, session) {
                    let _ = stream.write_all(b"{\"error\":\"403 self\"}\n");
                } else if session
                    .pane_by_id(pane_id)
                    .map(|p| p.is_service())
                    .unwrap_or(false)
                {
                    handle_send_to_service(pane_id, &data, &mut stream, session);
                } else {
                    match session.send_to_pane(pane_id, &data) {
                        Ok(()) => {
                            let _ = stream.write_all(b"{\"ok\":true}\n");
                        }
                        Err(e) => {
                            let _ = stream.write_all(format!("{{\"error\":\"{e}\"}}\n").as_bytes());
                        }
                    }
                }
            }
            ApiAction::Close {
                pane_id,
                mut stream,
            } => {
                if server::check_sid_protection(&stream, pane_id, session) {
                    let _ = stream.write_all(b"{\"error\":\"403 self\"}\n");
                } else {
                    match session.close_pane(pane_id) {
                        Ok(()) => {
                            let _ = stream.write_all(b"{\"ok\":true}\n");
                        }
                        Err(e) => {
                            let _ = stream.write_all(format!("{{\"error\":\"{e}\"}}\n").as_bytes());
                        }
                    }
                }
            }
            ApiAction::Wait {
                pane_id,
                pattern,
                timeout_secs,
                stream,
            } => {
                if session.pane_by_id(pane_id).is_none() {
                    let mut s = stream;
                    let _ = s.write_all(
                        format!("{{\"error\":\"pane not found: {pane_id}\"}}\n").as_bytes(),
                    );
                } else if session
                    .pane_by_id(pane_id)
                    .map(|p| p.is_service())
                    .unwrap_or(false)
                {
                    let mut s = stream;
                    let _ = s.write_all(
                        format!(
                            "{{\"error\":\"pane {pane_id} is a service pane; wait requires PTY pane\"}}\n"
                        )
                        .as_bytes(),
                    );
                } else {
                    let baseline = session.pane_line_count(pane_id);
                    srv.waiting_clients.push(server::WaitRequest {
                        stream,
                        pane_id,
                        pattern,
                        start: std::time::Instant::now(),
                        timeout_secs,
                        baseline_lines: baseline,
                    });
                }
            }
        }
    }
    Ok(())
}

fn handle_send_to_service(
    pane_id: u32,
    data: &[u8],
    stream: &mut std::os::unix::net::UnixStream,
    session: &mut Session,
) {
    use std::io::Write;
    let pane = match session.pane_by_id_mut(pane_id) {
        Some(p) => p,
        None => {
            let _ = stream
                .write_all(format!("{{\"error\":\"pane not found: {pane_id}\"}}\n").as_bytes());
            return;
        }
    };
    if !pane.service_accepts_input_from(0) {
        let _ = stream.write_all(
            format!("{{\"error\":\"pane {pane_id} service rejected input\"}}\n").as_bytes(),
        );
        return;
    }
    pane.deliver_service_input(crate::service::InputEvent::Bytes(data.to_vec()));
    let _ = stream.write_all(b"{\"ok\":true}\n");
}
