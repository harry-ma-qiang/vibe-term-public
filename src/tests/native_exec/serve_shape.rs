// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests that every serve reply carries the same normalized keys.

use super::{poll_watch_until_done, read_response, run_watch, setup_session};
use crate::commands::start_serve_no_pattern;
use std::os::unix::net::UnixStream;
use std::time::Duration;

fn assert_serve_shape(json: &serde_json::Value) {
    // why: every serve response must emit exactly the 6 normalized keys.
    assert!(json["pid"].is_number(), "pid must be a number: {json}");
    assert!(
        json["pane_id"].is_number(),
        "pane_id must be a number: {json}"
    );
    assert!(json["started"].is_boolean(), "started must be bool: {json}");
    assert!(json["alive"].is_boolean(), "alive must be bool: {json}");
    let ready = &json["ready"];
    assert!(
        ready.is_boolean() || ready.is_null(),
        "ready must be bool or null: {json}"
    );
    assert!(json["timeout"].is_boolean(), "timeout must be bool: {json}");
    let matched = &json["matched"];
    assert!(
        matched.is_string() || matched.is_null(),
        "matched must be string or null: {json}"
    );
    assert!(
        json.get("exit_code").is_none(),
        "serve must not emit exit_code (not meaningful for a service): {json}"
    );
    assert!(
        json.get("output").is_none(),
        "serve must not emit output (use read for that): {json}"
    );
}

#[test]
fn serve_no_pattern_emits_service_shape() {
    let mut session = setup_session();
    let (s1, mut s2) = UnixStream::pair().unwrap();
    s1.set_nonblocking(true).unwrap();
    s2.set_nonblocking(true).unwrap();
    start_serve_no_pattern("sleep 300", &mut session, s1);
    let json = read_response(&mut s2);
    assert_serve_shape(&json);
    assert_eq!(json["started"], true);
    assert!(
        json["ready"].is_null(),
        "ready must be null with no pattern: {json}"
    );
    assert_eq!(json["timeout"], false);
    assert!(
        json["matched"].is_null(),
        "matched must be null with no pattern: {json}"
    );
}

#[test]
fn serve_pattern_matched_emits_service_shape() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(
        &mut session,
        "echo 'Server listening on 8080'",
        "listening",
        10,
        false,
    );
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_serve_shape(&json);
    assert_eq!(json["started"], true);
    assert_eq!(json["ready"], true);
    assert_eq!(json["timeout"], false);
    assert!(
        json["matched"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("listening"),
        "matched should contain the pattern: {json}"
    );
}

#[test]
fn serve_pattern_timeout_emits_service_shape() {
    let mut session = setup_session();
    let (mut reqs, mut reader) =
        run_watch(&mut session, "sleep 60", "never_match_marker", 2, false);
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_serve_shape(&json);
    assert_eq!(json["started"], true);
    assert_eq!(json["ready"], false);
    assert_eq!(json["timeout"], true);
    assert!(
        json["matched"].is_null(),
        "matched must be null on timeout: {json}"
    );
}

#[test]
fn serve_child_died_reports_alive_false() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_watch(
        &mut session,
        "echo goodbye && exit 1",
        "never_match_marker",
        10,
        false,
    );
    poll_watch_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_serve_shape(&json);
    assert_eq!(json["started"], true);
    assert_eq!(json["alive"], false);
    assert_eq!(json["ready"], false);
    assert_eq!(json["timeout"], false);
    assert!(
        json["matched"].is_null(),
        "matched must be null when child died: {json}"
    );
}
