// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the session: spawning panes, resizing, closing, the chrome
//! buttons, the wait baseline, and the restart queue's argv validation.

use super::*;
use crate::filter::SecretFilter;
use std::thread;
use std::time::Duration;

#[test]
fn session_new_is_empty() {
    let s = Session::new(80, 24);
    assert_eq!(s.panes.len(), 0);
    assert_eq!(s.focused, 0);
    assert_eq!(s.sidebar_count, 0);
}

#[test]
fn add_main_spawns_child() {
    let mut s = Session::new(160, 45);
    s.add_main("echo", &["hello"], SecretFilter::new()).unwrap();
    assert_eq!(s.panes.len(), 1);
    assert!(s.panes[0].pty().child_pid.as_raw() > 0);
}

#[test]
fn add_sidebar_pane_with_layout() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 2;
    s.add_main("sleep", &["10"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("sleep", &["10"]).unwrap();
    s.add_sidebar_pane("sleep", &["10"]).unwrap();
    assert_eq!(s.panes.len(), 3);
    assert_ne!(s.panes[0].frame.outer.x, s.panes[1].frame.outer.x);
}

#[test]
fn handle_resize_updates_all_panes() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["10"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("sleep", &["10"]).unwrap();
    s.handle_resize(200, 60).unwrap();
    assert_eq!(s.term_cols, 200);
    let main_w = s.panes[0].frame.outer.w;
    let side_w = s.panes[1].frame.outer.w;
    assert_eq!(main_w + side_w, 200);
}

#[test]
fn status_bar_button_positions_found() {
    let bar = "Panes: [+] [x] \u{2502} Split: [1:4] [1:\u{03C6}] [1:1] [4:1] \u{2502} [D] [Q]";
    let pos = crate::status_bar::button_positions_line_1(bar);
    assert_eq!(
        pos.len(),
        8,
        "expected 8 buttons: 1 add + 1 close-idle + 4 splits + 1 detach + 1 quit"
    );
    assert!(matches!(pos[0].2, crate::status_bar::StatusAction::AddPane));
    assert!(matches!(
        pos[1].2,
        crate::status_bar::StatusAction::CloseIdle
    ));
    assert!(matches!(
        pos[2].2,
        crate::status_bar::StatusAction::Split(SplitRatio::OneToFour)
    ));
    assert!(matches!(
        pos[5].2,
        crate::status_bar::StatusAction::Split(SplitRatio::FourToOne)
    ));
    assert!(matches!(pos[6].2, crate::status_bar::StatusAction::Detach));
    assert!(matches!(pos[7].2, crate::status_bar::StatusAction::Quit));
}

#[test]
fn sidebar_running_count_detects_children() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["10"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("sleep", &["10"]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    let count = s.sidebar_running_count();
    assert!(count <= 1, "sidebar running count: {count}");
}

#[test]
fn close_pane_kills_child_process_group() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("sh", &["-c", "sleep 999 & sleep 999 & wait"])
        .unwrap();
    thread::sleep(Duration::from_millis(200));

    let child_pid = s.panes[1].pty().child_pid;
    let pgid = child_pid.as_raw();

    let alive_before = unsafe { libc::kill(-pgid, 0) } == 0;
    assert!(alive_before, "process group should be alive before close");

    s.close_pane(s.panes[1].id).unwrap();
    thread::sleep(Duration::from_millis(500));

    nix::sys::wait::waitpid(child_pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)).ok();
    thread::sleep(Duration::from_millis(100));

    let alive_after = unsafe { libc::kill(-pgid, 0) } == 0;
    assert!(!alive_after, "process group should be dead after close");
}

#[test]
fn kill_all_terminates_process_groups() {
    let mut s = Session::new(160, 45);
    s.add_main("sh", &["-c", "sleep 999 & wait"], SecretFilter::new())
        .unwrap();
    thread::sleep(Duration::from_millis(200));

    let child_pid = s.panes[0].pty().child_pid;
    let pgid = child_pid.as_raw();
    let alive_before = unsafe { libc::kill(-pgid, 0) } == 0;
    assert!(
        alive_before,
        "process group should be alive before kill_all"
    );

    s.kill_all();
    thread::sleep(Duration::from_millis(500));

    nix::sys::wait::waitpid(child_pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)).ok();
    thread::sleep(Duration::from_millis(100));

    let alive_after = unsafe { libc::kill(-pgid, 0) } == 0;
    assert!(!alive_after, "process group should be dead after kill_all");
}

#[test]
fn acquire_exec_pane_spawns_bash_c() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();

    let pane_id = s.acquire_exec_pane("echo exec_test_marker").unwrap();
    thread::sleep(Duration::from_millis(500));

    let pane = s.pane_by_id_mut(pane_id).unwrap();
    let _ = pane.feed_output();
    let text = pane.capture_full_text();
    assert!(
        text.contains("exec_test_marker"),
        "output should contain marker: {text}"
    );
    assert!(
        !text.contains("__vt"),
        "output should not contain __vt wrapper: {text}"
    );
    assert!(
        !text.contains("__S"),
        "output should not contain __S marker: {text}"
    );
    assert!(
        !text.contains("__E"),
        "output should not contain __E marker: {text}"
    );
}

#[test]
fn check_pattern_after_skips_stale_output() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("bash", &[]).unwrap();
    thread::sleep(Duration::from_millis(200));

    let pane_id = s.panes[1].id;

    s.send_to_pane(pane_id, b"echo STALE_MARKER\n").unwrap();
    thread::sleep(Duration::from_millis(300));
    let _ = s.panes[1].feed_output();

    let full_match = s.check_pattern(pane_id, "STALE_MARKER");
    assert!(
        full_match.is_some(),
        "check_pattern should find STALE_MARKER in full buffer"
    );

    let baseline = s.pane_line_count(pane_id);

    let after_match = s.check_pattern_after(pane_id, "STALE_MARKER", baseline);
    assert!(
        after_match.is_none(),
        "check_pattern_after should NOT match stale output"
    );
}

#[test]
fn check_pattern_after_finds_new_output() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("bash", &[]).unwrap();
    thread::sleep(Duration::from_millis(200));

    let pane_id = s.panes[1].id;

    s.send_to_pane(pane_id, b"echo OLD_DATA\n").unwrap();
    thread::sleep(Duration::from_millis(300));
    let _ = s.panes[1].feed_output();

    let baseline = s.pane_line_count(pane_id);

    s.send_to_pane(pane_id, b"echo FRESH_DATA\n").unwrap();
    thread::sleep(Duration::from_millis(300));
    let _ = s.panes[1].feed_output();

    let stale = s.check_pattern_after(pane_id, "OLD_DATA", baseline);
    assert!(stale.is_none(), "check_pattern_after should skip OLD_DATA");

    let fresh = s.check_pattern_after(pane_id, "FRESH_DATA", baseline);
    assert!(
        fresh.is_some(),
        "check_pattern_after should find FRESH_DATA"
    );
}

#[test]
fn status_bar_line_1_always_carries_add_close_detach_quit() {
    let bar_headless = crate::status_bar::build_text_line_1(false);
    let bar_fg = crate::status_bar::build_text_line_1(true);
    for bar in [&bar_headless, &bar_fg] {
        assert!(bar.contains("[+]"), "missing [+]: {bar}");
        assert!(bar.contains("[x]"), "missing [x]: {bar}");
        assert!(!bar.contains("[0x]"), "close-idle is spelled [x]: {bar}");
        assert!(bar.contains("[D]"), "missing always-[D]: {bar}");
        assert!(bar.contains("[Q]"), "missing [Q]: {bar}");
        for slot in ["[1x]", "[2x]", "[4x]", "[8x]"] {
            assert!(
                !bar.contains(slot),
                "{slot} is not a line 1 button any more: {bar}"
            );
        }
    }
}

#[test]
fn add_one_pane_spawns_exactly_one_shell() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 0;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    let panes_before = s.panes.len();
    let ok = s.add_one_pane();
    assert!(ok, "add_one_pane must succeed");
    assert_eq!(
        s.panes.len(),
        panes_before + 1,
        "exactly one pane added per click"
    );
}

#[test]
fn close_pane_if_idle_refuses_a_pane_running_a_subprocess() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("bash", &[], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("bash", &[]).unwrap();
    thread::sleep(Duration::from_millis(200));
    // why: a foreground subprocess makes the pane's pgrp diverge from its child.
    let pid = s.panes[1].id;
    s.send_to_pane(pid, b"sleep 60 &\nfg\n").unwrap();
    thread::sleep(Duration::from_millis(400));
    let refused = !s.close_pane_if_idle(pid);
    assert!(
        refused,
        "Active pane (bash with sleep subprocess) must NOT be closed by [x]"
    );
    assert!(
        s.pane_by_id(pid).is_some(),
        "active pane survives [x] click"
    );
}

#[test]
fn close_pane_if_idle_closes_a_completed_pane() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("bash", &[]).unwrap();
    thread::sleep(Duration::from_millis(200));
    let id = s.panes[1].id;
    s.mark_pane_completed(id, 0);
    thread::sleep(Duration::from_millis(100));
    let panes_before = s.panes.len();
    let ok = s.close_pane_if_idle(id);
    assert!(ok, "completed pane must close via [x]");
    assert_eq!(s.panes.len(), panes_before - 1);
    assert!(s.pane_by_id(id).is_none(), "pane removed from registry");
}

#[test]
fn close_idle_panes_skips_a_pane_running_a_subprocess() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 2;
    s.add_main("bash", &[], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("bash", &[]).unwrap();
    s.add_sidebar_pane("bash", &[]).unwrap();
    thread::sleep(Duration::from_millis(300));
    assert_eq!(s.panes.len(), 3);
    let active_id = s.panes[2].id;
    s.send_to_pane(active_id, b"sleep 60 &\nfg\n").unwrap();
    thread::sleep(Duration::from_millis(500));
    let idle_id = s.panes[1].id;
    let main_id = s.panes[0].id;
    let _ = main_id;
    let closed = s.close_idle_panes();
    assert!(
        !closed.is_empty(),
        "at least one idle pane should close; got {closed:?}"
    );
    assert!(
        s.pane_by_id(active_id).is_some(),
        "Active pane (bash + sleep subprocess) survives [0x]"
    );
    assert!(
        closed.contains(&idle_id) || closed.contains(&main_id),
        "an idle bash pane should be in the closed set"
    );
}

#[test]
fn is_closeable_by_chrome_tracks_subprocess_presence() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("bash", &[], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("bash", &[]).unwrap();
    thread::sleep(Duration::from_millis(200));
    let idle_id = s.panes[1].id;
    assert!(
        s.pane_by_id(idle_id).unwrap().is_closeable_by_chrome(),
        "Idle pane (bash at prompt) is closeable"
    );
    s.send_to_pane(idle_id, b"sleep 60 &\nfg\n").unwrap();
    thread::sleep(Duration::from_millis(400));
    assert!(
        !s.pane_by_id(idle_id).unwrap().is_closeable_by_chrome(),
        "Active pane (bash + sleep subprocess) NOT closeable"
    );
}

#[test]
fn detach_and_quit_button_positions_are_stable() {
    let bar_a = crate::status_bar::build_text_line_1(false);
    let bar_b = crate::status_bar::build_text_line_1(false);
    let pos_a = crate::status_bar::button_positions_line_1(&bar_a);
    let pos_b = crate::status_bar::button_positions_line_1(&bar_b);
    let d_a = pos_a
        .iter()
        .find(|(_, _, a)| matches!(a, crate::status_bar::StatusAction::Detach))
        .expect("[D]");
    let q_a = pos_a
        .iter()
        .find(|(_, _, a)| matches!(a, crate::status_bar::StatusAction::Quit))
        .expect("[Q]");
    let d_b = pos_b
        .iter()
        .find(|(_, _, a)| matches!(a, crate::status_bar::StatusAction::Detach))
        .expect("[D]");
    let q_b = pos_b
        .iter()
        .find(|(_, _, a)| matches!(a, crate::status_bar::StatusAction::Quit))
        .expect("[Q]");
    assert_eq!(d_a.0, d_b.0);
    assert_eq!(q_a.0, q_b.0);
}

#[test]
fn restart_queue_skips_invalid_argv() {
    // why: an unresolvable command must be skipped, not looped on as a bad shell.
    let mut s = Session::new(160, 45);
    s.add_main("sleep", &["1"], SecretFilter::new()).unwrap();
    let before = s.panes.len();
    s.restart_queue.push((
        std::time::Instant::now() - std::time::Duration::from_secs(1),
        "service:bridge".to_string(),
    ));
    s.restart_queue.push((
        std::time::Instant::now() - std::time::Duration::from_secs(1),
        "/nonexistent/binary".to_string(),
    ));
    s.process_restart_queue();
    assert_eq!(
        s.panes.len(),
        before,
        "invalid restart argv must not spawn a new pane"
    );
    assert!(
        s.restart_queue.is_empty(),
        "queue drained even when skipped"
    );
}

#[test]
fn quit_click_arms_the_toast_and_leaves_panes_running() {
    let mut s = Session::new(160, 45);
    s.add_main("sleep", &["999"], SecretFilter::new()).unwrap();
    let pid = s.panes[0].pty().child_pid;
    thread::sleep(Duration::from_millis(200));
    let alive_before = unsafe { libc::kill(pid.as_raw(), 0) } == 0;
    assert!(alive_before, "child must be alive before [Q]");

    s.arm_quit_toast();
    assert!(s.quit_toast_active(), "toast armed after [Q]");
    thread::sleep(Duration::from_millis(200));
    let alive_after = unsafe { libc::kill(pid.as_raw(), 0) } == 0;
    assert!(
        alive_after,
        "panes must survive [Q] click - only confirmation arms shutdown"
    );
    assert!(!s.shutdown_requested, "shutdown not yet requested");

    s.confirm_quit_yes();
    assert!(s.shutdown_requested, "[Y] click sets shutdown_requested");
    s.kill_all();
}

#[test]
fn closing_the_only_sidebar_regrows_main_and_clears_the_screen() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("sleep", &["60"]).unwrap();

    let split_main_w = s.panes[0].frame.outer.w;
    assert!(
        split_main_w < 160,
        "with a sidebar the main pane is narrower than the grid: {split_main_w}"
    );

    let side_id = s.panes[1].id;
    s.close_pane(side_id).unwrap();

    assert_eq!(s.panes.len(), 1, "sidebar removed");
    assert_eq!(s.sidebar_count, 0, "sidebar_count drops to 0 on close");
    let full_main_w = s.panes[0].frame.outer.w;
    assert_eq!(
        full_main_w, 160,
        "surviving main pane regrows to full grid width after close"
    );
    let (child_cols, _) = s.panes[0].frame.child_size();
    assert_eq!(
        s.panes[0].screen.cols, child_cols,
        "screen width tracks the regrown frame so render_full fills the row"
    );
    assert!(s.needs_redraw, "close_pane requests a redraw");

    let mut buf = Vec::new();
    s.composite_frame(&mut buf).unwrap();
    assert!(
        buf.windows(4).any(|w| w == b"\x1b[2J"),
        "composite redraw clears the screen, wiping the closed pane's region"
    );

    s.kill_all();
}
