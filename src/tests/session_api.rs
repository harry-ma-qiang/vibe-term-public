// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the wait pattern matcher: case folding, pipe alternatives, the
//! baseline that hides stale output, line-by-line matching, and for reading
//! back a pane the session has finished with.

use super::match_wait_pattern;
use crate::filter::SecretFilter;
use crate::frame::{Frame, Rect};
use crate::pane::Pane;
use crate::session::{MAX_FINISHED_PANES, Session};
use std::thread;
use std::time::Duration;

fn bare_pane(cmd: &str, args: &[&str]) -> Pane {
    let frame = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 60,
            h: 10,
        },
        0,
        0,
    );
    Pane::spawn(cmd, args, frame, SecretFilter::new()).unwrap()
}

#[test]
fn matches_single_substring_in_single_line() {
    let r = match_wait_pattern("all is OK now", "OK", 0);
    assert_eq!(r.as_deref(), Some("all is OK now"));
}

#[test]
fn matches_case_insensitive_by_default() {
    let r = match_wait_pattern("BUILD SUCCEEDED", "succeeded", 0);
    assert_eq!(r.as_deref(), Some("BUILD SUCCEEDED"));
}

#[test]
fn or_alternatives_with_pipe() {
    let r = match_wait_pattern("server is listening on 8080", "ready|listening", 0);
    assert_eq!(r.as_deref(), Some("server is listening on 8080"));
}

#[test]
fn returns_most_recent_match_when_multiple_lines_match() {
    let text = "first OK\nsecond OK\nthird OK";
    let r = match_wait_pattern(text, "OK", 0);
    assert_eq!(r.as_deref(), Some("third OK"));
}

#[test]
fn no_match_returns_none() {
    let r = match_wait_pattern("nothing of interest", "ready", 0);
    assert!(r.is_none());
}

#[test]
fn baseline_skips_lines_before_call() {
    let text = "stale OK\nstale OK\nfresh data\nfresh OK";
    let r = match_wait_pattern(text, "OK", 2);
    assert_eq!(r.as_deref(), Some("fresh OK"));
}

#[test]
fn baseline_excludes_old_matches() {
    let text = "match here\nno match\nno match";
    let r = match_wait_pattern(text, "match here", 1);
    assert!(r.is_none(), "old match before baseline must not fire");
}

#[test]
fn empty_pattern_returns_none() {
    let r = match_wait_pattern("anything", "", 0);
    assert!(r.is_none());
}

#[test]
fn empty_text_returns_none() {
    let r = match_wait_pattern("", "ready", 0);
    assert!(r.is_none());
}

#[test]
fn pipe_with_empty_alternatives_still_matches_others() {
    let r = match_wait_pattern("now listening", "ready||listening", 0);
    assert_eq!(r.as_deref(), Some("now listening"));
}

#[test]
fn substring_matches_across_word_boundaries() {
    let r = match_wait_pattern("preparing-ready-set-go", "ready", 0);
    assert_eq!(r.as_deref(), Some("preparing-ready-set-go"));
}

#[test]
fn multiline_pattern_does_not_match_across_lines() {
    let r = match_wait_pattern("foo\nbar", "foo bar", 0);
    assert!(r.is_none());
}

#[test]
fn unicode_match_lowercased_correctly() {
    let r = match_wait_pattern("DÉJÀ vu", "déjà", 0);
    assert_eq!(r.as_deref(), Some("DÉJÀ vu"));
}

#[test]
fn reading_a_reaped_pane_returns_the_output_it_had() {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("sh", &["-c", "echo REAPED_MARKER"])
        .unwrap();
    let id = s.panes[1].id;

    for _ in 0..100 {
        let _ = s.panes[1].feed_output();
        if !s.panes[1].is_alive() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = s.panes[1].feed_output();

    let mut out = Vec::new();
    assert!(s.reap_dead_panes(&mut out).unwrap(), "finished pane reaped");
    assert!(s.pane_by_id(id).is_none(), "pane is gone from the registry");

    let (total, text) = s.read_pane(id, 100).expect("reaped pane is still readable");
    assert!(total > 0, "line count survives the pane: {total}");
    assert!(
        text.contains("REAPED_MARKER"),
        "kept output holds what the command printed: {text}"
    );

    s.kill_all();
}

#[test]
fn read_pane_honours_max_lines_on_a_reaped_pane() {
    let mut s = Session::new(160, 45);
    let mut pane = bare_pane("sleep", &["60"]);
    pane.id = 7;
    pane.screen.feed(b"one\r\ntwo\r\nthree\r\n");
    s.remember_finished(&pane);
    pane.kill_process_tree();

    let (total, text) = s.read_pane(7, 2).unwrap();
    assert_eq!(total, 3, "total counts every non-empty line");
    assert_eq!(text, "two\nthree", "only the last max_lines are returned");
}

#[test]
fn the_finished_list_evicts_the_oldest_pane_at_the_cap() {
    let mut s = Session::new(160, 45);
    let mut pane = bare_pane("sleep", &["60"]);
    pane.screen.feed(b"kept output\r\n");
    let overflow = 3u32;
    for i in 0..(MAX_FINISHED_PANES as u32 + overflow) {
        pane.id = i;
        s.remember_finished(&pane);
    }
    pane.kill_process_tree();

    assert_eq!(
        s.finished.len(),
        MAX_FINISHED_PANES,
        "the list never grows past its cap"
    );
    for gone in 0..overflow {
        assert!(
            s.finished_text(gone).is_none(),
            "pane {gone} was evicted as the oldest"
        );
    }
    assert!(
        s.finished_text(overflow).is_some(),
        "the oldest surviving pane is still readable"
    );
    assert!(
        s.finished_text(MAX_FINISHED_PANES as u32 + overflow - 1)
            .is_some(),
        "the newest pane is readable"
    );
}

#[test]
fn reading_an_unknown_pane_id_is_an_error() {
    let s = Session::new(160, 45);
    let err = s.read_pane(999, 10).unwrap_err();
    assert_eq!(err, "pane not found: 999");
}

#[test]
fn a_reaped_pane_leaves_no_file_in_the_session_directory() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::new(160, 45);
    s.init_session_dir(dir.path()).unwrap();
    s.sidebar_count = 1;
    s.add_main("sleep", &["60"], SecretFilter::new()).unwrap();
    s.add_sidebar_pane("sh", &["-c", "echo NO_FILE_MARKER"])
        .unwrap();

    for _ in 0..100 {
        let _ = s.panes[1].feed_output();
        if !s.panes[1].is_alive() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let mut out = Vec::new();
    let _ = s.reap_dead_panes(&mut out);

    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        vec!["config.json".to_string()],
        "the only file a session writes is the config it was asked for: {entries:?}"
    );

    s.kill_all();
}
