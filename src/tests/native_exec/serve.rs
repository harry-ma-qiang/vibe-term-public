// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for serve replies: ready patterns, early exits, timeouts, no pattern.

use super::{poll_watch_until_done, read_response, run_watch, setup_session};
use crate::commands::{check_watch_requests, start_serve_no_pattern};
use std::io::sink;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn serve_ready_pattern_returns_ready_true() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(
        &mut session,
        "echo 'Server listening on port 8080'",
        "listening",
        10,
        false,
    );
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["ready"], true);
    assert!(json["pane_id"].is_number());
}

#[test]
fn serve_child_dies_before_pattern_returns_ready_false() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(
        &mut session,
        "echo goodbye && exit 1",
        "never_match_this_pattern",
        10,
        false,
    );
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["ready"], false);
    assert_eq!(
        json["alive"], false,
        "child died should report alive:false: {json}"
    );
    assert!(
        json.get("exit_code").is_none(),
        "serve should not emit exit_code: {json}"
    );
    assert!(
        json.get("output").is_none(),
        "serve should not emit output: {json}"
    );
}

#[test]
fn serve_fast_crash_survives_reap_and_captures_output() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(
        &mut session,
        "echo 'this should be captured' >&2; exit 1",
        "never-prints",
        10,
        false,
    );
    let pane_id = reqs[0].pane_id;
    // why: reap between polls, as the main loop does, so the race is exercised.
    let start = Instant::now();
    while !reqs.is_empty() && start.elapsed() < Duration::from_secs(10) {
        let _ = session.reap_dead_panes(&mut sink());
        check_watch_requests(&mut reqs, &mut session);
        if !reqs.is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    assert!(reqs.is_empty(), "watch request should complete, not hang");
    let json = read_response(&mut reader);
    assert_ne!(
        json["error"], "pane killed externally",
        "fast-crash serve must not report pane killed externally: {json}"
    );
    assert_eq!(
        json["alive"], false,
        "crashed child reports alive:false: {json}"
    );
    let pane = session.pane_by_id(pane_id);
    assert!(
        pane.is_some(),
        "serve pane must survive reap so output is inspectable"
    );
    let text = pane.unwrap().capture_full_text();
    assert!(
        text.contains("this should be captured"),
        "stderr must be captured before reap: {text:?}"
    );
}

#[test]
fn serve_timeout_returns_timeout_true_pane_alive() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(&mut session, "sleep 60", "never_match", 2, false);
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["ready"], false);
    assert_eq!(json["timeout"], true);
    let pane_id = json["pane_id"].as_u64().unwrap() as u32;
    let pane = session.pane_by_id(pane_id);
    assert!(
        pane.is_some(),
        "pane should still exist after serve timeout"
    );
    assert!(
        pane.unwrap().is_alive(),
        "serve timeout should NOT kill child"
    );
}

#[test]
fn serve_no_ready_pattern_returns_immediately() {
    let mut session = setup_session();
    let (s1, mut s2) = UnixStream::pair().unwrap();
    s1.set_nonblocking(true).unwrap();
    s2.set_nonblocking(true).unwrap();
    start_serve_no_pattern("sleep 300", &mut session, s1);
    let json = read_response(&mut s2);
    assert!(json["pane_id"].is_number());
    assert_eq!(json["started"], true);
    let pane_id = json["pane_id"].as_u64().unwrap() as u32;
    let pane = session.pane_by_id(pane_id);
    assert!(pane.is_some(), "pane should exist");
    assert!(pane.unwrap().is_alive(), "pane should be alive");
}

#[test]
fn serve_with_pipe_or_pattern() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(
        &mut session,
        "echo 'started on port 3000'",
        "listening|started",
        10,
        false,
    );
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["ready"], true, "pipe OR pattern should match: {json}");
}

#[test]
fn serve_case_insensitive_pattern() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(
        &mut session,
        "echo 'LISTENING on port 8080'",
        "listening",
        10,
        false,
    );
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(
        json["ready"], true,
        "case-insensitive match should work: {json}"
    );
}
