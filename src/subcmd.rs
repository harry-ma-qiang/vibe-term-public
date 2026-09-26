// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The client side: finding a session's socket, sending it one request and
//! returning the reply, stopping a session and removing dead ones.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) fn session_is_alive(dir: &std::path::Path) -> bool {
    let sock = dir.join("control.sock");
    if UnixStream::connect(&sock).is_ok() {
        return true;
    }
    let config_pid = std::fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok())
        .and_then(|j| j["pid"].as_i64())
        .unwrap_or(0) as i32;
    config_pid > 0 && crate::capture::pid_exists(config_pid)
}

/// Remove every `/tmp/vibe-term-*` directory whose session is no longer alive.
pub fn cmd_clean() {
    let mut removed = 0;
    if let Ok(entries) = std::fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("vibe-term-") {
                continue;
            }
            let dir = entry.path();
            if session_is_alive(&dir) {
                continue;
            }
            let _ = std::fs::remove_dir_all(&dir);
            println!("removed: {}", dir.display());
            removed += 1;
        }
    }
    if removed == 0 {
        println!("no stale sessions");
    }
}

/// Send SIGTERM to a session's daemon, then SIGKILL if it outlives five seconds.
pub fn cmd_stop(target: &str) -> io::Result<()> {
    if target.is_empty() {
        return Err(io::Error::other("usage: vibe-term stop <name|pid>"));
    }
    if let Some(sock) = crate::capture::resolve_session(target) {
        let dir = sock.parent().unwrap();
        let config = dir.join("config.json");
        let pid = std::fs::read_to_string(&config)
            .ok()
            .and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok())
            .and_then(|j| j["pid"].as_i64())
            .unwrap_or(0) as i32;
        if pid > 0 {
            let npid = nix::unistd::Pid::from_raw(pid);
            nix::sys::signal::kill(npid, nix::sys::signal::Signal::SIGTERM).ok();
            for _ in 0..50 {
                std::thread::sleep(Duration::from_millis(100));
                if !crate::capture::pid_exists(pid) {
                    break;
                }
            }
            if crate::capture::pid_exists(pid) {
                nix::sys::signal::kill(npid, nix::sys::signal::Signal::SIGKILL).ok();
            }
            let _ = std::fs::remove_file(&sock);
            println!("stopped pid:{pid}");
        }
        Ok(())
    } else {
        Err(io::Error::other(format!("session '{target}' not found")))
    }
}

/// Send one request to the daemon and print its reply.
pub fn run_api_client(args: &[String]) -> io::Result<()> {
    let response = run_api_client_capture(args)?;
    print!("{response}");
    Ok(())
}

pub(crate) fn absolutize_socket_path(p: &str) -> String {
    let path = std::path::Path::new(p);
    if path.is_absolute() {
        return p.to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(path).to_string_lossy().to_string(),
        Err(_) => p.to_string(),
    }
}

/// Send one tab-joined request to `VIBE_TERM_SOCKET`, else the first live socket.
pub fn run_api_client_capture(args: &[String]) -> io::Result<String> {
    let socket = std::env::var("VIBE_TERM_SOCKET").ok();
    let sock_path = match socket {
        Some(ref p) => absolutize_socket_path(p),
        None => {
            let socks = find_sockets()?;
            if socks.is_empty() {
                return Err(io::Error::other("no running vibe-term instance found"));
            }
            socks[0].to_string_lossy().to_string()
        }
    };

    let request = args.join("\t");
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
    let socket_timeout = compute_socket_timeout(cmd, &args[1..]);
    let mut stream = UnixStream::connect(&sock_path)
        .map_err(|e| io::Error::other(format!("connect to {sock_path}: {e}")))?;
    if let Some(t) = socket_timeout {
        stream.set_read_timeout(Some(Duration::from_secs(t)))?;
    } else {
        stream.set_read_timeout(None)?;
    }
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn compute_socket_timeout(cmd: &str, rest: &[String]) -> Option<u64> {
    let refs: Vec<&str> = rest.iter().map(|s| s.as_str()).collect();
    if let Ok(v) = std::env::var("VIBE_TERM_CLIENT_TIMEOUT") {
        let t = v.trim();
        if t.eq_ignore_ascii_case("none") || t == "0" {
            return None;
        }
        if let Ok(n) = t.parse::<u64>() {
            return Some(n);
        }
    }
    match cmd {
        "exec" => match crate::wire_parser::parse_exec_args(&refs) {
            Ok(parsed) if parsed.no_timeout => None,
            Ok(parsed) => Some(parsed.timeout_secs as u64 + 10),
            Err(_) => Some(10),
        },
        "serve" => match crate::wire_parser::parse_serve_args(&refs) {
            Ok(parsed) if parsed.no_timeout => None,
            Ok(parsed) => Some(parsed.timeout_secs as u64 + 10),
            Err(_) => Some(10),
        },
        "wait" => Some(120),
        _ => Some(5),
    }
}

/// The control socket for a named session, else the first live one found.
pub fn resolve_socket(target: Option<&str>) -> io::Result<String> {
    match target {
        Some(t) => match crate::capture::resolve_session(t) {
            Some(p) => Ok(p.to_string_lossy().to_string()),
            None => Err(io::Error::other(format!("session '{t}' not found"))),
        },
        None => {
            let socks = find_sockets()?;
            if socks.is_empty() {
                return Err(io::Error::other("no running vibe-term instance found"));
            }
            Ok(socks[0].to_string_lossy().to_string())
        }
    }
}

/// Every `/tmp/vibe-term-*/control.sock` that exists, in directory order.
pub fn find_sockets() -> io::Result<Vec<PathBuf>> {
    let mut sockets = Vec::new();
    for entry in std::fs::read_dir("/tmp")? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("vibe-term-") {
            let sock = entry.path().join("control.sock");
            if sock.exists() {
                sockets.push(sock);
            }
        }
    }
    Ok(sockets)
}

#[cfg(test)]
#[path = "tests/subcmd.rs"]
mod tests;
