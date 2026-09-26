// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the wait pattern matcher: case folding, pipe alternatives,
//! the baseline that hides stale output, and line-by-line matching.

use super::match_wait_pattern;

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
