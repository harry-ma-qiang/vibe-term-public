// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `send` and `read` wire verbs: argument parsing and the read reply.

use std::os::unix::net::UnixStream;

use super::{ApiAction, json_err, parse_key_name};

/// Decode `send <pane> <command|text|key> <value>` into the bytes to deliver.
pub fn parse_send(parts: &[&str], stream: UnixStream) -> ApiAction {
    let pane_id = match parts.first().and_then(|s| s.parse::<u32>().ok()) {
        Some(id) => id,
        None => {
            return ApiAction::Response(
                json_err("usage: send <pane> <command|text|key> <value>"),
                stream,
            );
        }
    };
    let mode = parts.get(1).copied().unwrap_or("");
    let value = parts[2..].join(" ");
    if value.is_empty() && mode != "key" {
        return ApiAction::Response(
            json_err("usage: send <pane> <command|text|key> <value>"),
            stream,
        );
    }
    match mode {
        "command" => {
            let mut data = value.into_bytes();
            data.push(b'\r');
            ApiAction::Send {
                pane_id,
                data,
                stream,
            }
        }
        "text" => ApiAction::Send {
            pane_id,
            data: value.into_bytes(),
            stream,
        },
        "key" => {
            let key_name = parts.get(2).copied().unwrap_or("");
            match parse_key_name(key_name) {
                Some(data) => ApiAction::Send {
                    pane_id,
                    data,
                    stream,
                },
                None => ApiAction::Response(json_err(&format!("unknown key: {key_name}")), stream),
            }
        }
        _ => ApiAction::Response(
            json_err("usage: send <pane> <command|text|key> <value>"),
            stream,
        ),
    }
}

/// Answer `read <pane> [lines] [search]` with a pane's last matching lines.
///
/// A pane that has ended is answered from the session's kept output.
pub fn cmd_read(args: &[&str], session: &crate::session::Session) -> String {
    let pane_id: u32 = match args.first().and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => return json_err("usage: read <pane> [lines] [search]"),
    };
    let lines: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let search = args.get(2).map(|s| s.to_string());

    match session.pane_by_id(pane_id) {
        Some(pane) => {
            let text = pane.capture_full_text();
            let all_lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
            let filtered: Vec<&str> = if let Some(ref pat) = search {
                let patterns: Vec<String> = pat.split('|').map(|p| p.to_lowercase()).collect();
                all_lines
                    .into_iter()
                    .filter(|l| {
                        let lower = l.to_lowercase();
                        patterns.iter().any(|p| lower.contains(p))
                    })
                    .collect()
            } else {
                all_lines
            };
            let total = filtered.len();
            let start = total.saturating_sub(lines);
            let shown = &filtered[start..];
            let output = shown.join("\n");
            super::json_ok(serde_json::json!({
                "pane": pane_id,
                "lines": shown.len(),
                "total": total,
                "output": output,
            }))
        }
        None => {
            if let Ok((_, content)) = session.read_pane(pane_id, usize::MAX) {
                let all_lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
                let filtered: Vec<&str> = if let Some(ref pat) = search {
                    let patterns: Vec<String> = pat.split('|').map(|p| p.to_lowercase()).collect();
                    all_lines
                        .into_iter()
                        .filter(|l| {
                            let lower = l.to_lowercase();
                            patterns.iter().any(|p| lower.contains(p))
                        })
                        .collect()
                } else {
                    all_lines
                };
                let total = filtered.len();
                let start = total.saturating_sub(lines);
                let shown = &filtered[start..];
                let output = shown.join("\n");
                return super::json_ok(serde_json::json!({
                    "pane": pane_id,
                    "closed": true,
                    "lines": shown.len(),
                    "total": total,
                    "output": output,
                }));
            }
            json_err(&format!("pane not found: {pane_id}"))
        }
    }
}
