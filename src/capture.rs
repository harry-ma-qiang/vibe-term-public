// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Wire requests: the parser for the public verbs, what a decoded request
//! asks the daemon to do, and the in-flight run and watch bookkeeping.

use std::os::unix::net::UnixStream;

mod keys;
mod parsers;
mod session;
mod status;

pub use keys::parse_key_name;
pub use parsers::cmd_read;
pub use session::{generate_session_name, resolve_session, session_name_exists};
pub use status::cmd_status;

/// Process liveness check that never sends a signal.
pub fn pid_exists(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let ret = unsafe { libc::kill(pid, 0) };
    ret == 0 || {
        let err = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::ESRCH);
        err != libc::ESRCH
    }
}

/// A decoded client request, paired with the stream that must be answered.
pub enum ApiAction {
    Response(String, UnixStream),
    Send {
        pane_id: u32,
        data: Vec<u8>,
        stream: UnixStream,
    },
    Close {
        pane_id: u32,
        stream: UnixStream,
    },
    Wait {
        pane_id: u32,
        pattern: String,
        timeout_secs: u32,
        stream: UnixStream,
    },
    Exec {
        cmd: String,
        timeout_secs: u32,
        no_timeout: bool,
        notify_pattern: Option<String>,
        stream: UnixStream,
    },
    Serve {
        cmd: String,
        pattern: Option<String>,
        timeout_secs: u32,
        no_timeout: bool,
        stream: UnixStream,
    },
}

/// A client waiting for one pane's command to finish.
pub struct RunRequest {
    pub stream: UnixStream,
    pub pane_id: u32,
    pub deadline: Option<std::time::Instant>,
    pub kill_on_timeout: bool,
    pub exit_status: Option<i32>,
    pub draining: bool,
}

/// A client waiting for a pattern to appear in one pane's output.
pub struct WatchRequest {
    pub stream: UnixStream,
    pub pane_id: u32,
    pub pattern: String,
    pub deadline: Option<std::time::Instant>,
    pub draining: bool,
}

/// Serialise a success payload as one newline-terminated json line.
pub fn json_ok(obj: serde_json::Value) -> String {
    serde_json::to_string(&obj).unwrap_or_else(|_| r#"{"error":"json failed"}"#.to_string()) + "\n"
}

/// Build a newline-terminated json error line from a message.
pub fn json_err(msg: &str) -> String {
    let escaped = msg.replace('\\', "\\\\").replace('"', "'");
    format!("{{\"error\":\"{escaped}\"}}\n")
}

/// Decode one request line into the action it asks for.
///
/// Arguments split on tabs when any are present, otherwise on whitespace.
pub fn parse_api_request(
    request: &str,
    session: &crate::session::Session,
    stream: UnixStream,
) -> ApiAction {
    let parts: Vec<&str> = if request.contains('\t') {
        request.split('\t').collect()
    } else {
        request.split_whitespace().collect()
    };
    match parts.first().copied() {
        Some("read") | Some("capture") => {
            ApiAction::Response(cmd_read(&parts[1..], session), stream)
        }
        Some("status") => ApiAction::Response(cmd_status(session), stream),
        Some("send") => parsers::parse_send(&parts[1..], stream),
        Some("close") => {
            if let Some(pane_id) = parts.get(1).and_then(|s| s.parse::<u32>().ok()) {
                ApiAction::Close { pane_id, stream }
            } else {
                ApiAction::Response(json_err("usage: close <pane>"), stream)
            }
        }
        Some("wait") => {
            if let Some(pane_id) = parts.get(1).and_then(|s| s.parse::<u32>().ok()) {
                let pattern = parts.get(2).unwrap_or(&"").to_string();
                let timeout = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);
                if pattern.is_empty() {
                    ApiAction::Response(json_err("usage: wait <pane> <pattern> [timeout]"), stream)
                } else {
                    ApiAction::Wait {
                        pane_id,
                        pattern,
                        timeout_secs: timeout,
                        stream,
                    }
                }
            } else {
                ApiAction::Response(json_err("usage: wait <pane> <pattern> [timeout]"), stream)
            }
        }
        Some("exec") => match crate::wire_parser::parse_exec_args(&parts[1..]) {
            Ok(parsed) => ApiAction::Exec {
                cmd: parsed.child_cmd,
                timeout_secs: parsed.timeout_secs,
                no_timeout: parsed.no_timeout,
                notify_pattern: parsed.notify_pattern,
                stream,
            },
            Err(e) => ApiAction::Response(json_err(&e.to_string()), stream),
        },
        Some("serve") => match crate::wire_parser::parse_serve_args(&parts[1..]) {
            Ok(parsed) => ApiAction::Serve {
                cmd: parsed.child_cmd,
                pattern: parsed.pattern,
                timeout_secs: parsed.timeout_secs,
                no_timeout: parsed.no_timeout,
                stream,
            },
            Err(e) => ApiAction::Response(json_err(&e.to_string()), stream),
        },
        _ => ApiAction::Response(
            json_err(&format!("unknown: {}", parts.first().unwrap_or(&""))),
            stream,
        ),
    }
}

#[cfg(test)]
#[path = "tests/capture.rs"]
mod tests;
