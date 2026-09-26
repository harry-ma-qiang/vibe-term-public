// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for reusing completed exec panes, including after a timeout kill.

use super::{poll_until_done, run_exec, setup_session};
use std::thread;
use std::time::Duration;

#[test]
fn pane_pool_reuses_completed_exec_panes() {
    let mut session = setup_session();

    let pane_id_a = session.acquire_exec_pane("echo first_exec").unwrap();
    thread::sleep(Duration::from_millis(500));
    if let Some(pane) = session.pane_by_id_mut(pane_id_a) {
        let _ = pane.feed_output();
    }
    session.mark_pane_completed(pane_id_a, 0);

    let pane_count_after_first = session.panes.len();

    let _pane_id_b = session.acquire_exec_pane("echo second_exec").unwrap();

    let pane_count_after_second = session.panes.len();
    assert_eq!(
        pane_count_after_first, pane_count_after_second,
        "pane pool should reuse completed pane slot, not grow: {} vs {}",
        pane_count_after_first, pane_count_after_second
    );
}

#[test]
fn timeout_killed_pane_returns_to_reusable_pool_state() {
    let mut session = setup_session();

    let (mut reqs, _reader) = run_exec(&mut session, "sleep 600", 2, false);
    let timed_out_id = reqs[0].pane_id;
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty(), "timeout must complete the request");

    let pane = session
        .pane_by_id(timed_out_id)
        .expect("timeout-killed pane is kept as a recyclable slot, not removed");
    assert!(
        pane.completed,
        "timeout-killed pane must be marked completed"
    );
    assert!(
        !pane.exec_managed,
        "timeout-killed pane must drop exec_managed so it is not pinned busy"
    );
    assert!(
        pane.is_completed(),
        "timeout-killed pane must report is_completed() for pool reuse"
    );

    let slot = pane.frame.slot;
    let count_before_reuse = session.panes.len();
    let reused_id = session.acquire_exec_pane("echo after_timeout").unwrap();
    assert_ne!(
        reused_id, timed_out_id,
        "recycle assigns a fresh monotonic pane id (ids never reused)"
    );
    assert_eq!(
        session.panes.len(),
        count_before_reuse,
        "next exec must recycle the timeout-killed slot in place, not grow the pool"
    );

    let recycled = session.pane_by_id(reused_id).unwrap();
    assert_eq!(
        recycled.frame.slot, slot,
        "recycled exec child takes over the timeout-killed pane's slot"
    );
    assert!(
        !recycled.completed && recycled.exec_managed,
        "recycled pane runs a fresh exec child (completed=false, exec_managed=true)"
    );

    thread::sleep(Duration::from_millis(100));
    session.kill_all();
}
