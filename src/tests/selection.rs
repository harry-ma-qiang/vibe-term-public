// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for pane-local selection: coordinate clamping, span containment,
//! soft-wrap rejoining in the copied text, and clipboard delivery.

use super::*;
use crate::frame::Rect;

fn rect() -> Rect {
    Rect {
        x: 10,
        y: 5,
        w: 20,
        h: 8,
    }
}

#[test]
fn clamp_inside_is_relative() {
    assert_eq!(clamp_to(&rect(), 12, 7), (2, 2));
}

#[test]
fn clamp_left_and_above_folds_to_origin() {
    assert_eq!(clamp_to(&rect(), 0, 0), (0, 0));
    assert_eq!(clamp_to(&rect(), 9, 4), (0, 0));
}

#[test]
fn clamp_right_and_below_folds_to_last_cell() {
    assert_eq!(clamp_to(&rect(), 500, 500), (19, 7));
}

#[test]
fn clamp_never_escapes_the_pane() {
    let r = rect();
    for x in 0..80 {
        for y in 0..40 {
            let (c, rw) = clamp_to(&r, x, y);
            assert!(c < r.w, "col {c} escaped width {}", r.w);
            assert!(rw < r.h, "row {rw} escaped height {}", r.h);
        }
    }
}

#[test]
fn single_row_selection_contains_only_its_span() {
    let mut s = Selection::start(0, 0, 2, 1);
    s.extend_to(5, 1);
    assert!(!s.contains(1, 1));
    assert!(s.contains(2, 1));
    assert!(s.contains(5, 1));
    assert!(!s.contains(6, 1));
    assert!(!s.contains(3, 0));
}

#[test]
fn backwards_drag_is_normalized() {
    let mut s = Selection::start(0, 0, 5, 1);
    s.extend_to(2, 1);
    assert_eq!(s.ordered(), ((2, 1), (5, 1)));
    assert!(s.contains(3, 1));
}

#[test]
fn multi_row_selection_spans_full_middle_rows() {
    let mut s = Selection::start(0, 0, 4, 1);
    s.extend_to(3, 3);
    assert!(!s.contains(3, 1));
    assert!(s.contains(4, 1));
    assert!(s.contains(0, 2));
    assert!(s.contains(99, 2));
    assert!(s.contains(3, 3));
    assert!(!s.contains(4, 3));
}

#[test]
fn plain_click_selects_nothing() {
    let s = Selection::start(0, 0, 3, 3);
    assert!(s.is_empty());
}

#[test]
fn osc52_wraps_base64_payload() {
    let out = String::from_utf8(osc52("hi")).unwrap();
    assert_eq!(out, "\x1b]52;c;aGk=\x07");
}

fn screen_with(cols: usize, rows: usize, feed: &[u8]) -> crate::emulator::Screen {
    let mut s = crate::emulator::Screen::new(cols, rows);
    s.feed(feed);
    s
}

#[test]
fn wrapped_line_is_joined_without_newline() {
    let sc = screen_with(10, 4, b"ABCDEFGHIJKLMNO");
    assert!(sc.wrapped[0]);
    let mut s = Selection::start(0, 0, 0, 0);
    s.extend_to(4, 1);
    assert_eq!(s.text(&sc), "ABCDEFGHIJKLMNO");
}

#[test]
fn hard_newline_is_preserved() {
    let sc = screen_with(20, 4, b"first\r\nsecond");
    assert!(!sc.wrapped[0]);
    let mut s = Selection::start(0, 0, 0, 0);
    s.extend_to(5, 1);
    assert_eq!(s.text(&sc), "first\nsecond");
}

#[test]
fn partial_wrapped_selection_still_joins() {
    let sc = screen_with(10, 4, b"ABCDEFGHIJKLMNO");
    let mut s = Selection::start(0, 0, 3, 0);
    s.extend_to(2, 1);
    assert_eq!(s.text(&sc), "DEFGHIJKLM");
}

#[test]
fn selection_text_never_contains_a_border_glyph() {
    let sc = screen_with(10, 3, b"LEFTPANE!!");
    let rect = Rect {
        x: 0,
        y: 0,
        w: 10,
        h: 3,
    };
    let mut s = Selection::start(0, 0, 0, 0);
    let (c, r) = clamp_to(&rect, 999, 999);
    s.extend_to(c, r);
    let out = s.text(&sc);
    assert!(!out.contains('\u{2502}'), "border leaked: {out:?}");
    assert!(out.starts_with("LEFTPANE!!"));
}

#[test]
fn host_clipboard_helper_roundtrip() {
    use std::process::Command;
    let have = Command::new("which")
        .arg("pbcopy")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have {
        return;
    }
    let saved = Command::new("pbpaste").output().map(|o| o.stdout).ok();
    let marker = "vibe-term-clipboard-selftest-9f3a";
    assert!(copy_to_host_clipboard(marker), "helper reported failure");
    let got = Command::new("pbpaste").output().expect("pbpaste").stdout;
    let got = String::from_utf8_lossy(&got).to_string();
    if let Some(prev) = saved {
        let _ = copy_to_host_clipboard(&String::from_utf8_lossy(&prev));
    }
    assert_eq!(got, marker, "clipboard did not receive the copied text");
}
