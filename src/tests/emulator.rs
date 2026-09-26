// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the emulated screen: text, cursor, colours, alt screen,
//! scrollback, charsets, wrapping and resize.

use super::*;

#[test]
fn simple_text_input() {
    let mut s = Screen::new(80, 24);
    s.feed(b"Hello, world!");
    assert_eq!(s.row_text(0), "Hello, world!");
    assert_eq!(s.cursor_col, 13);
}

#[test]
fn newline_and_carriage_return() {
    let mut s = Screen::new(80, 24);
    s.feed(b"line1\r\nline2");
    assert_eq!(s.row_text(0), "line1");
    assert_eq!(s.row_text(1), "line2");
}

#[test]
fn cursor_movement() {
    let mut s = Screen::new(80, 24);
    s.feed(b"ABCDE");
    s.feed(b"\x1b[1;1H");
    s.feed(b"X");
    assert_eq!(s.row_text(0), "XBCDE");
}

#[test]
fn clear_line_right() {
    let mut s = Screen::new(80, 24);
    s.feed(b"Hello, world!");
    s.feed(b"\x1b[1;6H");
    s.feed(b"\x1b[K");
    assert_eq!(s.row_text(0), "Hello");
}

#[test]
fn clear_screen_all() {
    let mut s = Screen::new(80, 24);
    s.feed(b"stuff");
    s.feed(b"\x1b[2J");
    assert_eq!(s.row_text(0), "");
}

#[test]
fn scroll_on_overflow() {
    let mut s = Screen::new(80, 3);
    s.feed(b"line1\r\nline2\r\nline3\r\nline4");
    assert_eq!(s.row_text(0), "line2");
    assert_eq!(s.row_text(1), "line3");
    assert_eq!(s.row_text(2), "line4");
}

#[test]
fn dirty_tracking() {
    let mut s = Screen::new(80, 24);
    s.clear_dirty();
    s.feed(b"\x1b[5;1HX");
    assert!(!s.dirty[0]);
    assert!(s.dirty[4]);
}

#[test]
fn wide_char_handling() {
    let mut s = Screen::new(80, 24);
    s.feed("你好".as_bytes());
    assert_eq!(s.row_text(0), "你好");
    assert_eq!(s.cursor_col, 4);
    assert_eq!(s.cells[0][0].width, 2);
    assert_eq!(s.cells[0][1].width, 0);
}

#[test]
fn ansi_escape_not_in_buffer() {
    let mut s = Screen::new(80, 24);
    s.feed(b"\x1b[31mRED\x1b[0m");
    assert_eq!(s.row_text(0), "RED");
    assert!(!s.cells[0][0].style.bold);
}

#[test]
fn hidden_text_still_in_buffer() {
    let mut s = Screen::new(80, 24);
    s.feed(b"\x1b[8mSECRET\x1b[0m");
    assert_eq!(s.row_text(0), "SECRET");
    assert!(s.cells[0][0].style.hidden);
}

#[test]
fn resize_screen() {
    let mut s = Screen::new(80, 24);
    s.feed(b"Hello");
    s.resize(40, 12);
    assert_eq!(s.cols, 40);
    assert_eq!(s.rows, 12);
    assert_eq!(s.row_text(0), "Hello");
}

#[test]
fn default_fg_reset_via_sgr39() {
    let mut s = Screen::new(80, 24);
    s.feed(b"\x1b[31mRED\x1b[39mDEFAULT");
    assert_eq!(s.cells[0][0].style.fg, TermColor::Indexed(1));
    assert_eq!(s.cells[0][3].style.fg, TermColor::Default);
}

#[test]
fn default_bg_reset_via_sgr49() {
    let mut s = Screen::new(80, 24);
    s.feed(b"\x1b[41mRED_BG\x1b[49mDEFAULT");
    assert_eq!(s.cells[0][0].style.bg, TermColor::Indexed(1));
    assert_eq!(s.cells[0][6].style.bg, TermColor::Default);
}

#[test]
fn alt_screen_saves_and_restores() {
    let mut s = Screen::new(80, 24);
    s.feed(b"Primary content");
    assert_eq!(s.row_text(0), "Primary content");
    s.feed(b"\x1b[?1049h");
    assert_eq!(s.row_text(0), "");
    s.feed(b"Alt content");
    assert_eq!(s.row_text(0), "Alt content");
    s.feed(b"\x1b[?1049l");
    assert_eq!(s.row_text(0), "Primary content");
}

#[test]
fn scrollback_captures_scrolled_lines() {
    let mut s = Screen::new(80, 3);
    s.feed(b"line1\r\nline2\r\nline3\r\nline4");
    assert_eq!(s.scrollback_len(), 1);
    assert_eq!(s.scrollback_row_text(0).unwrap(), "line1");
    assert_eq!(s.row_text(0), "line2");
}

#[test]
fn scrollback_grows_with_output() {
    let mut s = Screen::new(80, 3);
    for i in 1..=10 {
        s.feed(format!("line{i}\r\n").as_bytes());
    }
    assert_eq!(s.scrollback_len(), 8);
    assert_eq!(s.scrollback_row_text(0).unwrap(), "line1");
    assert_eq!(s.scrollback_row_text(7).unwrap(), "line8");
}

#[test]
fn scrollback_capped_at_max() {
    let mut s = Screen::new(80, 3);
    for i in 1..=2010 {
        s.feed(format!("L{i}\r\n").as_bytes());
    }
    assert_eq!(s.scrollback_len(), MAX_SCROLLBACK);
    assert_eq!(s.scrollback_row_text(0).unwrap(), "L9");
}

#[test]
fn scrollback_not_saved_in_alt_screen() {
    let mut s = Screen::new(80, 3);
    s.feed(b"line1\r\nline2\r\nline3");
    assert_eq!(s.scrollback_len(), 0);
    s.feed(b"\x1b[?1049h");
    s.feed(b"alt1\r\nalt2\r\nalt3\r\nalt4\r\nalt5");
    assert_eq!(s.scrollback_len(), 0);
    s.feed(b"\x1b[?1049l");
    assert_eq!(s.scrollback_len(), 0);
}

#[test]
fn total_lines_includes_scrollback() {
    let mut s = Screen::new(80, 3);
    s.feed(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
    assert_eq!(s.total_lines(), s.scrollback_len() + 3);
}

#[test]
fn scrollback_row_text_returns_lines() {
    let mut s = Screen::new(80, 3);
    s.feed(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
    assert!(s.scrollback_len() >= 2);
    let first = s.scrollback_row_text(0).unwrap();
    assert_eq!(first, "line1");
    let second = s.scrollback_row_text(1).unwrap();
    assert_eq!(second, "line2");
}

#[test]
fn scrollback_full_history() {
    let mut s = Screen::new(80, 3);
    for i in 0..50 {
        s.feed(format!("line{i}\r\n").as_bytes());
    }
    assert!(s.scrollback_len() >= 47);
    let first = s.scrollback_row_text(0).unwrap();
    assert_eq!(first, "line0");
}

#[test]
fn acs_so_si_switching() {
    let mut s = Screen::new(80, 24);
    // why: ESC ) 0 designates G1 as special graphics; SO selects it, SI restores G0.
    s.feed(b"\x1b)0");
    s.feed(b"A");
    s.feed(b"\x0e");
    s.feed(b"l");
    s.feed(b"\x0f");
    s.feed(b"B");
    assert_eq!(s.row_text(0), "A┌B");
}

#[test]
fn acs_esc_paren_0_designates_g0() {
    let mut s = Screen::new(80, 24);
    s.feed(b"\x1b(0");
    s.feed(b"lqk");
    s.feed(b"\x1b(B");
    s.feed(b"lqk");
    assert_eq!(s.row_text(0), "┌─┐lqk");
}

#[test]
fn acs_full_mapping_table() {
    let mut s = Screen::new(80, 24);
    s.feed(b"\x1b(0");
    let mappings: &[(u8, char)] = &[
        (b'l', '┌'),
        (b'q', '─'),
        (b'x', '│'),
        (b'k', '┐'),
        (b'm', '└'),
        (b'j', '┘'),
        (b't', '├'),
        (b'u', '┤'),
        (b'n', '┼'),
        (b'w', '┬'),
        (b'v', '┴'),
        (b'a', '▒'),
    ];
    for &(byte, expected) in mappings {
        s.feed(b"\x1b[2J\x1b[H");
        s.feed(&[byte]);
        let got = s.cells[0][0].ch;
        assert_eq!(
            got, expected,
            "ACS byte 0x{:02x} '{}' -> expected '{}', got '{}'",
            byte, byte as char, expected, got
        );
    }
}

#[test]
fn acs_does_not_affect_unicode_box_drawing() {
    let mut s = Screen::new(80, 24);
    s.feed("┌─┐│└┘".as_bytes());
    assert_eq!(s.row_text(0), "┌─┐│└┘");
}

#[test]
fn acs_reset_on_esc_paren_b() {
    let mut s = Screen::new(80, 24);
    s.feed(b"\x1b(0");
    s.feed(b"l");
    s.feed(b"\x1b(B");
    s.feed(b"l");
    assert_eq!(s.row_text(0), "┌l");
}

#[test]
fn wrapped_flag_set_on_overflow() {
    let mut s = Screen::new(10, 3);
    s.feed(b"1234567890AB");
    assert!(s.wrapped[0]);
    assert!(!s.wrapped[1]);
    assert_eq!(s.row_text(0), "1234567890");
    assert_eq!(s.row_text(1), "AB");
}

#[test]
fn wrapped_flag_cleared_by_linefeed() {
    let mut s = Screen::new(10, 3);
    s.feed(b"1234567890AB\r\nCD");
    assert!(s.wrapped[0]);
    assert!(!s.wrapped[1]);
    assert_eq!(s.row_text(2), "CD");
}

#[test]
fn scrollback_preserves_wrapped_flag() {
    let mut s = Screen::new(10, 3);
    s.feed(b"1234567890ABCDEFGHIJ\r\nline3\r\nline4\r\nline5");
    assert!(s.scrollback_len() >= 2);
    assert!(s.scrollback_is_wrapped(0));
    assert_eq!(s.scrollback_row_text(0).unwrap(), "1234567890");
    assert_eq!(s.scrollback_row_text(1).unwrap(), "ABCDEFGHIJ");
    assert!(!s.scrollback_is_wrapped(1));
}

#[test]
fn scrollback_wrapped_cap_matches_scrollback() {
    let mut s = Screen::new(10, 3);
    for i in 0..2010 {
        s.feed(format!("L{i:06}\r\n").as_bytes());
    }
    assert_eq!(s.scrollback_len(), MAX_SCROLLBACK);
    assert_eq!(s.scrollback_wrapped.len(), MAX_SCROLLBACK);
}

#[test]
fn resize_grow_pads_scrollback_rows() {
    let mut s = Screen::new(10, 3);
    for i in 0..5 {
        s.feed(format!("line{i:02}    \r\n").as_bytes());
    }
    let sb_before = s.scrollback_len();
    assert!(
        sb_before >= 2,
        "preflight: scrollback populated, got {sb_before}"
    );
    for idx in 0..sb_before {
        let row = s.scrollback_row(idx).expect("row");
        assert_eq!(row.len(), 10, "rows are stored at original 10-col width");
    }

    s.resize(40, 3);
    assert_eq!(s.scrollback_len(), sb_before, "no rows lost on resize");
    for idx in 0..s.scrollback_len() {
        let row = s.scrollback_row(idx).expect("row");
        assert!(
            row.len() >= 40,
            "scrollback row {idx} must be padded to >= new cols 40, got {}",
            row.len()
        );
    }
}

#[test]
fn resize_shrink_does_not_truncate_scrollback() {
    // why: shrink must not lose history; a later grow re-reveals the content.
    let mut s = Screen::new(20, 3);
    for i in 0..5 {
        s.feed(format!("WIDETEXT{i:02}        \r\n").as_bytes());
    }
    let row0_before = s.scrollback_row(0).map(|r| r.len()).unwrap_or(0);
    s.resize(10, 3);
    let row0_after_shrink = s.scrollback_row(0).map(|r| r.len()).unwrap_or(0);
    assert_eq!(
        row0_before, row0_after_shrink,
        "shrink must NOT truncate scrollback (data loss)"
    );
    s.resize(20, 3);
    let row0_after_regrow = s.scrollback_row(0).map(|r| r.len()).unwrap_or(0);
    assert!(
        row0_after_regrow >= 20,
        "regrow padded back to >= 20, got {row0_after_regrow}"
    );
}

#[test]
fn autowrap_disabled_clamps_at_last_column() {
    let mut s = Screen::new(10, 4);
    s.feed(b"\x1b[?7l");
    s.feed(b"ABCDEFGHIJKLMN");
    assert_eq!(s.row_text(0), "ABCDEFGHIN");
    assert_eq!(s.row_text(1), "");
    assert_eq!(s.cursor_row, 0);
}

#[test]
fn autowrap_enabled_still_wraps() {
    let mut s = Screen::new(10, 4);
    s.feed(b"ABCDEFGHIJKL");
    assert_eq!(s.row_text(0), "ABCDEFGHIJ");
    assert_eq!(s.row_text(1), "KL");
    assert_eq!(s.cursor_row, 1);
}

#[test]
fn autowrap_can_be_re_enabled() {
    let mut s = Screen::new(10, 4);
    s.feed(b"\x1b[?7l");
    s.feed(b"\x1b[?7h");
    s.feed(b"ABCDEFGHIJKL");
    assert_eq!(s.row_text(1), "KL");
}
