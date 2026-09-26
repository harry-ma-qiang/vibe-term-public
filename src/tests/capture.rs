// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the request parser: each public verb, unknown and retired
//! verbs, the error shape, and reading a pane after it is closed.

use super::*;
use std::os::unix::net::UnixStream;

fn test_api(request: &str, session: &crate::session::Session) -> String {
    let (s1, _s2) = UnixStream::pair().unwrap();
    let action = parse_api_request(request, session, s1);
    match action {
        ApiAction::Response(msg, _) => msg,
        _ => "action".to_string(),
    }
}

#[test]
fn handle_unknown_command() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("bogus", &session);
    assert!(resp.contains("error"), "should error: {resp}");
    assert!(resp.contains("unknown"), "should say unknown: {resp}");
}

#[test]
fn handle_status_json() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("status", &session);
    assert!(resp.contains("\"focused\""), "status: {resp}");
    assert!(resp.contains("\"panes\""), "status: {resp}");
    assert!(resp.contains("\"session\""), "status: {resp}");
    let parsed: serde_json::Value = serde_json::from_str(resp.trim()).expect("valid JSON");
    assert!(parsed["panes"].is_array());
}

#[test]
fn handle_read_missing_pane() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("read 0", &session);
    assert!(resp.contains("error"), "should error: {resp}");
    assert!(resp.contains("not found"), "should say not found: {resp}");
}

#[test]
fn handle_read_with_pane() {
    let mut session = crate::session::Session::new(80, 24);
    session
        .add_main(
            "echo",
            &["hello_capture"],
            crate::filter::SecretFilter::new(),
        )
        .unwrap();
    // why: pty startup is async, so under parallel load poll with retries.
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = session.panes[0].feed_output();
        let resp = test_api("read 0", &session);
        if resp.contains("hello_capture") {
            let parsed: serde_json::Value = serde_json::from_str(resp.trim()).expect("valid JSON");
            assert!(parsed["pane"].as_u64() == Some(0));
            assert!(parsed["output"].as_str().unwrap().contains("hello_capture"));
            return;
        }
    }
    panic!("hello_capture not found after retries");
}

#[test]
fn handle_status_with_pane() {
    let mut session = crate::session::Session::new(160, 45);
    session.sidebar_count = 1;
    session
        .add_main("sleep", &["10"], crate::filter::SecretFilter::new())
        .unwrap();
    let resp = test_api("status", &session);
    let parsed: serde_json::Value = serde_json::from_str(resp.trim()).expect("valid JSON");
    assert!(parsed["panes"][0]["pane"].as_u64() == Some(0));
    assert!(parsed["panes"][0]["pid"].is_number());
    assert!(parsed["panes"][0]["state"].is_string());
}

#[test]
fn handle_send_returns_action() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("send 0 command ls", &session);
    assert_eq!(resp, "action");
}

#[test]
fn handle_exec_returns_action() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("exec echo hello", &session);
    assert_eq!(resp, "action");
}

#[test]
fn exec_with_vt_timeout_parses() {
    let session = crate::session::Session::new(80, 24);
    let (s1, _s2) = UnixStream::pair().unwrap();
    let action = parse_api_request("exec --vt-timeout 10 echo hello", &session, s1);
    match action {
        ApiAction::Exec {
            cmd,
            timeout_secs,
            no_timeout,
            ..
        } => {
            assert_eq!(cmd, "echo hello");
            assert_eq!(timeout_secs, 10);
            assert!(!no_timeout);
        }
        _ => panic!("expected Exec action"),
    }
}

#[test]
fn exec_default_timeout() {
    let session = crate::session::Session::new(80, 24);
    let (s1, _s2) = UnixStream::pair().unwrap();
    let action = parse_api_request("exec echo hello", &session, s1);
    match action {
        ApiAction::Exec {
            cmd,
            timeout_secs,
            no_timeout,
            ..
        } => {
            assert_eq!(cmd, "echo hello");
            assert_eq!(timeout_secs, 300);
            assert!(!no_timeout);
        }
        _ => panic!("expected Exec action"),
    }
}

#[test]
fn exec_no_args_returns_error() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("exec", &session);
    assert!(resp.contains("error"), "should error: {resp}");
    assert!(resp.contains("no command"), "should say no command: {resp}");
}

#[test]
fn handle_serve_returns_action() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("serve npm listening", &session);
    assert_eq!(resp, "action");
}

#[test]
fn handle_key_parse() {
    assert!(parse_key_name("ctrl-c").is_some());
    assert!(parse_key_name("enter").is_some());
    assert!(parse_key_name("bogus").is_none());
}

#[test]
fn old_commands_rejected() {
    let session = crate::session::Session::new(80, 24);
    for cmd in &[
        "cmd 0 ls",
        "type 0 hello",
        "key 0 enter",
        "kill 0",
        "state",
        "context",
        "quit",
    ] {
        let resp = test_api(cmd, &session);
        assert!(
            resp.contains("error") || resp.contains("unknown"),
            "old command '{cmd}' should be rejected: {resp}"
        );
    }
}

#[test]
fn json_error_format() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("bogus", &session);
    let parsed: serde_json::Value = serde_json::from_str(resp.trim()).expect("valid JSON");
    assert!(parsed["error"].is_string());
}

#[test]
fn close_returns_action_with_pane_id() {
    let session = crate::session::Session::new(80, 24);
    let (s1, _s2) = UnixStream::pair().unwrap();
    match parse_api_request("close 3", &session, s1) {
        ApiAction::Close { pane_id, .. } => assert_eq!(pane_id, 3),
        _ => panic!("expected Close action"),
    }
}

#[test]
fn close_without_pane_is_usage_error() {
    let session = crate::session::Session::new(80, 24);
    let resp = test_api("close", &session);
    assert!(resp.contains("usage: close <pane>"), "usage: {resp}");
    let resp = test_api("close abc", &session);
    assert!(resp.contains("usage: close <pane>"), "usage: {resp}");
}

#[test]
fn removed_verbs_are_unknown() {
    let session = crate::session::Session::new(80, 24);
    for cmd in &["spawn bash", "layout ratio 1:1", "ls", "lock status"] {
        let resp = test_api(cmd, &session);
        assert!(
            resp.contains("unknown"),
            "'{cmd}' should be unknown: {resp}"
        );
    }
}

#[test]
fn a_closed_pane_stays_readable() {
    let mut session = crate::session::Session::new(160, 45);
    session.sidebar_count = 1;
    session
        .add_main(
            "sh",
            &["-c", "echo closed_pane_marker; sleep 30"],
            crate::filter::SecretFilter::new(),
        )
        .unwrap();
    let mut seen = false;
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = session.panes[0].feed_output();
        if session.panes[0]
            .capture_full_text()
            .contains("closed_pane_marker")
        {
            seen = true;
            break;
        }
    }
    assert!(seen, "marker never reached the pane");
    let (s1, _s2) = UnixStream::pair().unwrap();
    let pane_id = match parse_api_request("close 0", &session, s1) {
        ApiAction::Close { pane_id, .. } => pane_id,
        _ => panic!("expected Close action"),
    };
    session.close_pane(pane_id).unwrap();
    assert!(session.pane_by_id(0).is_none(), "pane must be gone");
    let resp = test_api("read 0", &session);
    let parsed: serde_json::Value = serde_json::from_str(resp.trim()).expect("valid JSON");
    assert_eq!(parsed["closed"], true, "read after close: {resp}");
    assert!(
        parsed["output"]
            .as_str()
            .unwrap()
            .contains("closed_pane_marker"),
        "closed pane output: {resp}"
    );
}
