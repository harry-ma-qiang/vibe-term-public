// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Finding a session's control socket by name or pid, and naming new sessions.

use std::path::PathBuf;

/// The control socket of a live session named `name_or_pid`, or its daemon pid.
pub fn resolve_session(name_or_pid: &str) -> Option<PathBuf> {
    let sock = PathBuf::from(format!("/tmp/vibe-term-{name_or_pid}/control.sock"));
    if sock.exists() {
        return Some(sock);
    }
    if !name_or_pid.contains('-')
        && let Some(user) = current_user_prefix()
    {
        let prefixed = format!("/tmp/vibe-term-{user}-{name_or_pid}/control.sock");
        let sock_prefixed = PathBuf::from(prefixed);
        if sock_prefixed.exists() {
            return Some(sock_prefixed);
        }
    }
    if let Ok(entries) = std::fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let dir_name = entry.file_name();
            let dir_str = dir_name.to_string_lossy();
            if dir_str.starts_with("vibe-term-") {
                let config = entry.path().join("config.json");
                if let Ok(data) = std::fs::read_to_string(&config)
                    && let Ok(json) = serde_json::from_str::<serde_json::Value>(&data)
                {
                    let matches_pid = name_or_pid
                        .parse::<i64>()
                        .ok()
                        .map(|p| json["pid"].as_i64() == Some(p))
                        .unwrap_or(false);
                    if matches_pid {
                        let sock = entry.path().join("control.sock");
                        if sock.exists() {
                            return Some(sock);
                        }
                    }
                }
            }
        }
    }
    None
}

/// A random two-word session name with a numeric suffix, unused at the time.
pub fn generate_session_name() -> String {
    const ADJ: &[&str] = &[
        "based", "cozy", "chad", "smol", "mega", "turbo", "hyper", "ultra", "dank", "epic",
        "cursed", "blessed", "sigma", "alpha", "omega", "pixel", "cyber", "retro", "vapor", "neon",
        "giga", "yolo", "stonks", "hodl",
    ];
    const NOUN: &[&str] = &[
        "wojak", "pepe", "doge", "bonk", "monke", "fren", "anon", "npc", "chad", "doomer",
        "bloomer", "zoomer", "goose", "honk", "boop", "stonk", "hodler", "ape", "tendies", "vibes",
        "copium", "hopium", "kek",
    ];
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as usize;
    let pid = std::process::id() as usize;
    let a = ADJ[(seed ^ pid) % ADJ.len()];
    let n = NOUN[(seed.wrapping_mul(7) ^ pid) % NOUN.len()];
    let short = (pid % 100) as u32;
    let name = format!("{a}-{n}-{short:02}");
    if session_name_exists(&name) {
        format!("{a}-{n}-{}", pid % 10000)
    } else {
        name
    }
}

/// Whether a session by this name already has a control socket.
pub fn session_name_exists(name: &str) -> bool {
    let sock = PathBuf::from(format!("/tmp/vibe-term-{name}/control.sock"));
    sock.exists()
}

fn current_user_prefix() -> Option<String> {
    std::env::var("USER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::process::Command::new("whoami")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

#[cfg(test)]
#[path = "../tests/resolve_session.rs"]
mod tests;
