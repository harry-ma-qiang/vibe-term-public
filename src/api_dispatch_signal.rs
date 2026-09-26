// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `signal` wire verb: a strict safelist of signal names, the parsing
//! around it, and delivery to one pane's child or its process group.

use crate::session::Session;

/// Signal a pane's child, or its whole group, and answer on the stream.
///
/// The name may carry a case-insensitive `SIG` prefix.
pub fn handle_signal(
    pane_id: u32,
    name: &str,
    tree: bool,
    stream: &mut std::os::unix::net::UnixStream,
    session: &Session,
) {
    use std::io::Write;
    let sig = match parse_signal_name(name) {
        Some(s) => s,
        None => {
            let _ =
                stream.write_all(format!("{{\"error\":\"unknown signal: {name}\"}}\n").as_bytes());
            return;
        }
    };
    let pane = match session.pane_by_id(pane_id) {
        Some(p) => p,
        None => {
            let _ = stream
                .write_all(format!("{{\"error\":\"pane not found: {pane_id}\"}}\n").as_bytes());
            return;
        }
    };
    let pty = match pane.pty_opt() {
        Some(p) => p,
        None => {
            let _ = stream.write_all(
                format!(
                    "{{\"error\":\"pane {pane_id} is a service pane; signal requires PTY pane\"}}\n"
                )
                .as_bytes(),
            );
            return;
        }
    };
    let pid = pty.child_pid.as_raw();
    let result = if tree {
        unsafe { libc::kill(-pid, sig as i32) }
    } else {
        unsafe { libc::kill(pid, sig as i32) }
    };
    let canonical = normalise_signal_name(strip_sig_prefix(name));
    if result == 0 {
        let _ = stream.write_all(
            format!(
                "{{\"ok\":true,\"pane\":{pane_id},\"pid\":{pid},\"signal\":\"{canonical}\",\"tree\":{tree}}}\n"
            )
            .as_bytes(),
        );
    } else {
        let err = std::io::Error::last_os_error();
        let _ = stream.write_all(format!("{{\"error\":\"kill failed: {err}\"}}\n").as_bytes());
    }
}

/// The only signal names accepted, case-sensitive and without a prefix.
///
/// Signals that are easy to misuse are deliberately left out.
pub const SIGNAL_SAFELIST: &[&str] = &["HUP", "INT", "QUIT", "TERM", "USR1", "USR2", "KILL"];

/// Drop a leading case-insensitive `SIG` prefix from a signal name.
///
/// A bare `SIG` is left intact so it fails the safelist check.
pub fn strip_sig_prefix(name: &str) -> &str {
    if name.len() > 3 && name[..3].eq_ignore_ascii_case("SIG") {
        &name[3..]
    } else {
        name
    }
}

/// Map a safelisted signal name to its signal, or None.
///
/// The bare name must be uppercase; only the `SIG` prefix may vary.
pub fn parse_signal_name(name: &str) -> Option<nix::sys::signal::Signal> {
    use nix::sys::signal::Signal::*;
    let name = strip_sig_prefix(name);
    if !SIGNAL_SAFELIST.contains(&name) {
        return None;
    }
    match name {
        "HUP" => Some(SIGHUP),
        "INT" => Some(SIGINT),
        "QUIT" => Some(SIGQUIT),
        "TERM" => Some(SIGTERM),
        "USR1" => Some(SIGUSR1),
        "USR2" => Some(SIGUSR2),
        "KILL" => Some(SIGKILL),
        _ => None,
    }
}

/// Put the `SIG` prefix back for the response envelope.
pub fn normalise_signal_name(name: &str) -> String {
    format!("SIG{name}")
}

#[cfg(test)]
#[path = "tests/api_dispatch_signal.rs"]
mod tests;
