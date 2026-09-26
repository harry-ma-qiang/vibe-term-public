// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `status` wire verb: the session's layout and one envelope per pane.

use super::json_ok;

/// Answer `status` with the session's layout and every pane's state.
pub fn cmd_status(session: &crate::session::Session) -> String {
    let panes: Vec<serde_json::Value> = session
        .panes
        .iter()
        .map(|p| {
            let (w, h) = p.frame.child_size();
            let state_str = if p.completed {
                "completed"
            } else if p.is_idle() {
                "idle"
            } else {
                "active"
            };
            let text = p.capture_text();
            let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
            let last_lines: Vec<&str> = lines
                .into_iter()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let pid = p.pty_opt().map(|pty| pty.child_pid.as_raw()).unwrap_or(0);
            let mut envelope = serde_json::json!({
                "pane": p.id,
                "kind": p.kind.label(),
                "pid": pid,
                "cmd": p.original_cmd,
                "state": state_str,
                "alive": p.is_alive(),
                "cpu": (p.stats.cpu_percent * 10.0).round() / 10.0,
                "mem_mb": p.stats.mem_mb.round(),
                "cols": w,
                "rows": h,
                "lines": last_lines,
            });
            if let Some(name) = p.service_name() {
                envelope["service_name"] = serde_json::Value::String(name.to_string());
            }
            if let Some(status) = p.service_status_json()
                && let serde_json::Value::Object(map) = status
                && let Some(obj) = envelope.as_object_mut()
            {
                // why: a service's own status must never overwrite the pane's envelope fields.
                const CORE_ENVELOPE_KEYS: &[&str] = &[
                    "pid",
                    "cmd",
                    "state",
                    "alive",
                    "cpu",
                    "mem_mb",
                    "cols",
                    "rows",
                    "lines",
                    "log_file",
                    "pane",
                    "kind",
                    "service_name",
                ];
                for (k, v) in map {
                    if CORE_ENVELOPE_KEYS.contains(&k.as_str()) {
                        continue;
                    }
                    obj.insert(k, v);
                }
            }
            envelope
        })
        .collect();
    let session_dir = session
        .session_dir
        .as_ref()
        .map(|d| d.display().to_string());
    let audit_log = session
        .session_dir
        .as_ref()
        .map(|d| d.join("session.log").display().to_string());
    json_ok(serde_json::json!({
        "session": session.name,
        "focused": session.focused,
        "sidebar_count": session.sidebar_count,
        "ratio": session.split_ratio.label(),
        "filter": session.filter_enabled,
        "panes": panes,
        "session_dir": session_dir,
        "audit_log": audit_log,
        "total_panes": session.panes.len(),
        "browse": "disabled",
    }))
}
