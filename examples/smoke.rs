// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Drives the emulator with real terminal output and prints what it renders.
//!
//! A unit test says an assertion held. This says what a person would have seen,
//! which is how the rendering faults in this code were found in the first place.
//! Run it with `cargo run --example smoke` and read the output.

use vibe_term::emulator::Screen;

fn show(label: &str, cols: usize, rows: usize, bytes: &[u8]) {
    let mut s = Screen::new(cols, rows);
    s.feed(bytes);
    println!("--- {label}  ({cols}x{rows}, alt={})", s.is_alt_screen());
    for r in 0..rows {
        println!("   |{}|", s.row_text(r));
    }
    println!(
        "    scrollback={} total_lines={}",
        s.scrollback_len(),
        s.total_lines()
    );
}

fn main() {
    show("plain text", 20, 3, b"hello\r\nworld\r\n");
    show(
        "colour and bold, which must not print as text",
        20,
        3,
        b"\x1b[1;31mRED BOLD\x1b[0m\r\nplain\r\n",
    );
    show(
        "cursor home, erase line, overwrite",
        20,
        3,
        b"aaaaa\r\nbbbbb\r\n\x1b[H\x1b[KXX",
    );
    show(
        "forward five, erase to end of line",
        20,
        3,
        b"123456789\r\x1b[5C\x1b[K",
    );
    show("wrap at the column edge", 8, 3, b"abcdefghij");
    show(
        "scrolling pushes rows into scrollback",
        10,
        2,
        b"one\r\ntwo\r\nthree\r\nfour\r\n",
    );
    show(
        "switch to the alternate screen",
        12,
        2,
        b"main\r\n\x1b[?1049h",
    );
    show(
        "wide characters keep their width",
        12,
        2,
        "中文abc\r\n".as_bytes(),
    );
    show(
        "carriage return with no newline overwrites",
        12,
        2,
        b"xxxxxxx\rab",
    );
    show(
        "absolute cursor position then write",
        12,
        3,
        b"\x1b[2;3Hhere",
    );

    let mut s = Screen::new(10, 3);
    s.feed(b"1111111111\r\n2222222222\r\n3333333333\r\n4444\r\n");
    println!("--- narrow to 6 columns and back to 10");
    println!(
        "   before  scrollback={} row0={:?}",
        s.scrollback_len(),
        s.row_text(0)
    );
    s.resize(6, 3);
    println!(
        "   narrow  scrollback={} row0={:?}",
        s.scrollback_len(),
        s.row_text(0)
    );
    s.resize(10, 3);
    println!(
        "   wide    scrollback={} row0={:?}",
        s.scrollback_len(),
        s.row_text(0)
    );
}
