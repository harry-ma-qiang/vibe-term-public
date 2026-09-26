// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the diffing renderer: text, cursor, styles and resize.

use super::*;
use crate::emulator::Screen;

fn output_to_string(screen: &mut Screen) -> String {
    let mut renderer = Renderer::new(screen.cols, screen.rows);
    let mut buf = Vec::new();
    renderer.render_full(screen, &mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

#[test]
fn render_simple_text() {
    let mut screen = Screen::new(20, 3);
    screen.feed(b"Hello");
    let out = output_to_string(&mut screen);
    assert!(out.contains("Hello"), "missing text: {out}");
}

#[test]
fn render_cursor_position() {
    let mut screen = Screen::new(20, 3);
    screen.feed(b"AB");
    let out = output_to_string(&mut screen);
    assert!(
        out.contains("\x1b[1;3H"),
        "cursor should be at row=1,col=3: {out}"
    );
}

#[test]
fn render_bold_text() {
    let mut screen = Screen::new(20, 3);
    screen.feed(b"\x1b[1mBOLD\x1b[0m");
    let out = output_to_string(&mut screen);
    assert!(out.contains("\x1b[1m"), "missing bold code: {out}");
    assert!(out.contains("BOLD"), "missing text: {out}");
}

#[test]
fn render_colored_text() {
    let mut screen = Screen::new(20, 3);
    screen.feed(b"\x1b[31mRED\x1b[0m");
    let out = output_to_string(&mut screen);
    assert!(out.contains("\x1b[31m"), "missing red code: {out}");
}

#[test]
fn diff_only_redraws_dirty() {
    let mut screen = Screen::new(20, 5);
    screen.feed(b"line1\r\nline2\r\nline3");

    let mut renderer = Renderer::new(20, 5);
    let mut buf = Vec::new();
    renderer.render_full(&mut screen, &mut buf).unwrap();
    buf.clear();

    screen.feed(b"\x1b[2;1HchangeD");
    renderer.render_diff(&mut screen, &mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();

    assert!(out.contains("changeD"), "dirty row missing: {out}");
    assert!(!out.contains("line1"), "clean row redrawn: {out}");
    assert!(!out.contains("line3"), "clean row redrawn: {out}");
}

#[test]
fn diff_skips_unchanged() {
    let mut screen = Screen::new(20, 3);
    screen.feed(b"static");

    let mut renderer = Renderer::new(20, 3);
    let mut buf = Vec::new();
    renderer.render_full(&mut screen, &mut buf).unwrap();
    buf.clear();

    renderer.render_diff(&mut screen, &mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();
    assert!(!out.contains("static"), "unchanged content redrawn: {out}");
}

#[test]
fn render_wide_chars() {
    let mut screen = Screen::new(20, 3);
    screen.feed("你好".as_bytes());
    let out = output_to_string(&mut screen);
    assert!(out.contains("你好"), "missing wide chars: {out}");
}

#[test]
fn render_reset_between_styles() {
    let mut screen = Screen::new(40, 3);
    screen.feed(b"\x1b[1mBOLD\x1b[0m NORMAL");
    let out = output_to_string(&mut screen);
    assert!(out.contains("\x1b[0m"), "missing reset: {out}");
}

#[test]
fn resize_clears_prev_state() {
    let mut screen = Screen::new(20, 3);
    screen.feed(b"Hello");
    let mut renderer = Renderer::new(20, 3);
    let mut buf = Vec::new();
    renderer.render_full(&mut screen, &mut buf).unwrap();

    renderer.resize(40, 5);
    assert_eq!(renderer.cols, 40);
    assert_eq!(renderer.rows, 5);
    assert_eq!(renderer.prev.len(), 5);
    assert_eq!(renderer.prev[0].len(), 40);
}
