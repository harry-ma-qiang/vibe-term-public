// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for reading, sending to and waiting on exec panes.

use super::{poll_until_done, run_exec, setup_session};
use std::thread;
use std::time::Duration;

#[test]
fn read_command_on_exec_pane_works() {
    let mut session = setup_session();
    let (mut reqs, _reader) = run_exec(&mut session, "echo read_test_data", 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));

    for pane in &mut session.panes {
        let _ = pane.feed_output();
    }

    let mut found_pane = false;
    for pane in &session.panes {
        let (total, output) = session.read_pane(pane.id, 30).unwrap_or((0, String::new()));
        if output.contains("read_test_data") {
            found_pane = true;
            assert!(total > 0);
        }
    }
    assert!(found_pane, "should find pane with read_test_data");
}

#[test]
fn send_delivers_input_to_child_stdin() {
    let mut session = setup_session();
    let pane_id = session.acquire_exec_pane("cat").unwrap();
    thread::sleep(Duration::from_millis(200));

    session.send_to_pane(pane_id, b"hello_from_send\n").unwrap();
    thread::sleep(Duration::from_millis(500));

    if let Some(pane) = session.pane_by_id_mut(pane_id) {
        let _ = pane.feed_output();
    }
    let text = session.pane_by_id(pane_id).unwrap().capture_full_text();
    assert!(
        text.contains("hello_from_send"),
        "cat should echo input: {text}"
    );
}

#[test]
fn wait_on_pane_detects_new_output() {
    let mut session = setup_session();
    let pane_id = session.acquire_exec_pane("bash").unwrap();
    thread::sleep(Duration::from_millis(200));

    if let Some(pane) = session.pane_by_id_mut(pane_id) {
        let _ = pane.feed_output();
    }
    let baseline = session.pane_line_count(pane_id);

    session
        .send_to_pane(pane_id, b"echo WAIT_MARKER\n")
        .unwrap();
    thread::sleep(Duration::from_millis(500));
    if let Some(pane) = session.pane_by_id_mut(pane_id) {
        let _ = pane.feed_output();
    }

    let found = session.check_pattern_after(pane_id, "WAIT_MARKER", baseline);
    assert!(found.is_some(), "should detect WAIT_MARKER after baseline");
}
