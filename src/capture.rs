// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Wire action and request types: what a decoded client request asks the
//! daemon to do, plus the in-flight run and watch bookkeeping.

use std::os::unix::net::UnixStream;

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
    Runtime {
        ns: &'static str,
        verb: String,
        args: Vec<String>,
        stream: UnixStream,
    },
    RuntimeCancel {
        stream_id: u64,
        stream: UnixStream,
    },
    Send {
        pane_id: u32,
        data: Vec<u8>,
        stream: UnixStream,
    },
    Spawn {
        cmd: String,
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
    Layout {
        action: LayoutAction,
        stream: UnixStream,
    },
    Signal {
        pane_id: u32,
        signal: String,
        tree: bool,
        stream: UnixStream,
    },
    Resize {
        pane_id: u32,
        cols: u16,
        rows: u16,
        stream: UnixStream,
    },
    Lock {
        op: String,
        stream: UnixStream,
    },
    Service {
        action: ServiceAction,
        stream: UnixStream,
    },
    Browse {
        action: BrowseAction,
        stream: UnixStream,
    },
}

/// A change to the pane grid: split ratio, focus, sidebar count or cleanup.
pub enum LayoutAction {
    Ratio(String),
    Focus(u32),
    Sidebar(usize),
    CleanIdle,
}

/// An operation on the set of long-running service panes.
pub enum ServiceAction {
    Spawn(String),
    List,
    Kill(u32),
    Restart(u32),
}

/// An operation on the attached browser control channel.
pub enum BrowseAction {
    Status,
    Send(String),
    Restart,
    Config,
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
