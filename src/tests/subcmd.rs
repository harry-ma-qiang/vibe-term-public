// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for client read deadlines, socket path pinning and stale session detection.

use super::{absolutize_socket_path, compute_socket_timeout, session_is_alive};
use std::fs;
use std::os::unix::net::UnixListener;

#[test]
fn unknown_and_short_verbs_keep_default_timeout() {
    assert_eq!(compute_socket_timeout("status", &[]), Some(5));
    let read_args: Vec<String> = vec!["7".into()];
    assert_eq!(compute_socket_timeout("read", &read_args), Some(5));
}

#[test]
fn exec_and_serve_deadline_is_their_timeout_plus_ten() {
    let exec: Vec<String> = vec!["--vt-timeout".into(), "20".into(), "true".into()];
    assert_eq!(compute_socket_timeout("exec", &exec), Some(30));
    let serve: Vec<String> = vec!["--vt-timeout".into(), "0".into(), "x".into(), "p".into()];
    assert_eq!(compute_socket_timeout("serve", &serve), None);
    assert_eq!(compute_socket_timeout("wait", &[]), Some(120));
}

#[test]
fn absolute_socket_path_unchanged() {
    assert_eq!(
        absolutize_socket_path("/tmp/vibe-term-x/control.sock"),
        "/tmp/vibe-term-x/control.sock"
    );
}

#[test]
fn relative_socket_path_pinned_to_cwd() {
    let out = absolutize_socket_path("control.sock");
    let path = std::path::Path::new(&out);
    assert!(
        path.is_absolute(),
        "relative path must be absolutized: {out}"
    );
    assert!(out.ends_with("control.sock"));
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("vibe-term-cleant-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn dead_label_session_no_pid_is_stale() {
    let d = tmp("nopid");
    fs::write(d.join("config.json"), br#"{"name":"tb"}"#).unwrap();
    assert!(
        !session_is_alive(&d),
        "no socket + no pid => stale (reapable)"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn dead_session_with_dead_pid_is_stale() {
    let d = tmp("deadpid");
    fs::write(d.join("config.json"), br#"{"name":"tb","pid":2147480000}"#).unwrap();
    assert!(!session_is_alive(&d), "no socket + dead pid => stale");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn live_socket_session_is_alive() {
    let d = tmp("live");
    let _l = UnixListener::bind(d.join("control.sock")).unwrap();
    assert!(
        session_is_alive(&d),
        "a LISTENING control.sock => alive (never reaped)"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn busy_daemon_live_pid_no_socket_is_kept() {
    let d = tmp("busy");
    fs::write(
        d.join("config.json"),
        format!(r#"{{"name":"tb","pid":{}}}"#, std::process::id()).as_bytes(),
    )
    .unwrap();
    assert!(
        session_is_alive(&d),
        "live pid (this test proc) => kept even with no socket"
    );
    let _ = fs::remove_dir_all(&d);
}
