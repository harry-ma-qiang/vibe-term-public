// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for exec timeout parsing through the request parser, and deadlines.

use super::{poll_until_done, read_response, run_exec, setup_session};
use crate::commands::start_run;
use crate::session::Session;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn exec_zero_args_returns_error_json() {
    let session = Session::new(160, 45);
    let (s1, _s2) = UnixStream::pair().unwrap();
    let action = crate::capture::parse_api_request("exec", &session, s1);
    match action {
        crate::capture::ApiAction::Response(msg, _) => {
            let json: serde_json::Value = serde_json::from_str(msg.trim()).unwrap();
            assert!(json["error"].is_string());
            assert!(
                json["error"].as_str().unwrap().contains("no command"),
                "error should mention no command: {}",
                json["error"]
            );
        }
        _ => panic!("exec with no args should return Response, not action"),
    }
}

#[test]
fn vt_timeout_zero_means_no_timeout() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_exec(&mut session, "echo no_timeout_test", 0, true);
    assert_eq!(reqs.len(), 1);
    assert!(
        reqs[0].deadline.is_none(),
        "no_timeout should have no deadline"
    );
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["timeout"], false);
}

#[test]
fn invalid_vt_timeout_returns_error_json() {
    let session = Session::new(160, 45);
    let (s1, _s2) = UnixStream::pair().unwrap();
    let action =
        crate::capture::parse_api_request("exec --vt-timeout abc echo hello", &session, s1);
    match action {
        crate::capture::ApiAction::Response(msg, _) => {
            let json: serde_json::Value = serde_json::from_str(msg.trim()).unwrap();
            assert!(json["error"].is_string());
            assert!(
                json["error"].as_str().unwrap().contains("invalid"),
                "should mention invalid: {}",
                json["error"]
            );
        }
        _ => panic!("invalid timeout should return error Response"),
    }
}

#[test]
fn negative_vt_timeout_returns_error_json() {
    let session = Session::new(160, 45);
    let (s1, _s2) = UnixStream::pair().unwrap();
    let action = crate::capture::parse_api_request("exec --vt-timeout -5 echo hello", &session, s1);
    match action {
        crate::capture::ApiAction::Response(msg, _) => {
            let json: serde_json::Value = serde_json::from_str(msg.trim()).unwrap();
            assert!(json["error"].is_string());
        }
        _ => panic!("negative timeout should return error Response"),
    }
}

#[test]
fn default_timeout_300s_applies() {
    let mut session = setup_session();
    let (s1, _s2) = UnixStream::pair().unwrap();
    s1.set_nonblocking(true).unwrap();
    let mut srv = crate::server::Server::start_named("test").unwrap();
    let req = start_run("sleep 999", 300, false, None, &mut session, s1, &mut srv);
    assert!(req.is_some());
    let r = req.unwrap();
    assert!(r.deadline.is_some());
    let deadline = r.deadline.unwrap();
    let expected_range = Duration::from_secs(295)..Duration::from_secs(305);
    let remaining = deadline.duration_since(Instant::now());
    assert!(
        expected_range.contains(&remaining),
        "default deadline should be ~300s from now, got {:.1}s",
        remaining.as_secs_f64()
    );
}
