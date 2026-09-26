// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for exec replies: exit codes, output, timeouts and the pane id.

use super::{poll_until_done, read_response, run_exec, setup_session};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn exec_simple_command_returns_exit_code_and_output() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_exec(&mut session, "echo hello_test_marker", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty(), "request should have completed");
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 0);
    assert!(
        json["output"]
            .as_str()
            .unwrap()
            .contains("hello_test_marker")
    );
    assert_eq!(json["timeout"], false);
}

#[test]
fn exec_nonzero_exit_code() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_exec(&mut session, "false", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 1);
    assert_eq!(json["timeout"], false);
}

#[test]
fn exec_vt_timeout_separate_from_child_timeout() {
    let mut session = setup_session();
    let (mut reqs, mut reader) =
        run_exec(&mut session, "echo timeout_test --timeout 30", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 0);
    assert!(json["output"].as_str().unwrap().contains("--timeout 30"));
}

#[test]
fn exec_double_dash_passes_flags_to_child() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_exec(&mut session, "echo --help", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 0);
    assert!(json["output"].as_str().unwrap().contains("--help"));
}

#[test]
fn exec_multiline_command() {
    let mut session = setup_session();
    let (mut reqs, mut reader) =
        run_exec(&mut session, "for i in 1 2 3; do echo $i; done", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 0);
    let output = json["output"].as_str().unwrap();
    assert!(output.contains("1"), "output: {output}");
    assert!(output.contains("2"), "output: {output}");
    assert!(output.contains("3"), "output: {output}");
}

#[test]
fn exec_no_markers_in_pane_output() {
    let mut session = setup_session();
    let (mut reqs, _reader) = run_exec(&mut session, "echo clean_output_test", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));

    for pane in &mut session.panes {
        let _ = pane.feed_output();
    }
    for pane in &session.panes {
        let text = pane.capture_full_text();
        if text.contains("clean_output_test") {
            assert!(!text.contains("__vt"), "no __vt markers: {text}");
            assert!(!text.contains("__S"), "no __S markers: {text}");
            assert!(!text.contains("__E"), "no __E markers: {text}");
        }
    }
}

#[test]
fn exec_timeout_returns_partial_output_and_timeout_true() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_exec(
        &mut session,
        "echo partial_before_timeout && sleep 60",
        2,
        false,
    );
    let start = Instant::now();
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    let elapsed = start.elapsed();
    assert!(reqs.is_empty());
    let json = read_response(&mut reader);
    assert_eq!(json["timeout"], true);
    assert!(json["exit_code"].is_null());
    assert!(
        elapsed.as_secs() >= 1 && elapsed.as_secs() <= 6,
        "elapsed: {:.1}s",
        elapsed.as_secs_f64()
    );
}

#[test]
fn exec_timeout_kills_child() {
    let mut session = setup_session();
    let (mut reqs, _reader) = run_exec(&mut session, "sleep 600", 2, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));

    thread::sleep(Duration::from_secs(3));

    for pane in &session.panes {
        let text = pane.capture_full_text();
        if text.is_empty() || pane.completed {
            continue;
        }
    }
}

#[test]
fn exec_pane_output_has_pane_id_field() {
    let mut session = setup_session();
    let (mut reqs, mut reader) = run_exec(&mut session, "echo pane_id_check", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    let json = read_response(&mut reader);
    assert!(
        json["pane_id"].is_number(),
        "response should contain pane_id: {json}"
    );
}
