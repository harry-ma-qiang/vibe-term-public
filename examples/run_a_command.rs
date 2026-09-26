// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Spawns a real process on a real pty and prints what the pane captured.
//!
//! This is the whole point of the library in one file: a command runs, its
//! output goes through the emulator, and a program reads the text back.
//! Run it with `cargo run --example run_a_command`.
//!
//! Every block reports `completed=false` even after the command has exited.
//! That is expected here and explained in docs/.

use std::io;
use std::thread::sleep;
use std::time::{Duration, Instant};
use vibe_term::filter::SecretFilter;
use vibe_term::frame::{Frame, Rect};
use vibe_term::pane::Pane;

fn drain(pane: &mut Pane, budget: Duration) -> io::Result<()> {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        pane.feed_output()?;
        if pane.is_completed() {
            break;
        }
        sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn run(label: &str, cmd: &str, args: &[&str]) -> io::Result<()> {
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
    let mut pane = Pane::spawn(cmd, args, frame, SecretFilter::default())?;
    drain(&mut pane, Duration::from_secs(3))?;
    println!("--- {label}");
    println!(
        "    completed={} idle={}",
        pane.is_completed(),
        pane.is_idle()
    );
    println!("    lines={}", pane.line_count());
    for line in pane
        .capture_full_text()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        println!("    | {line}");
    }
    Ok(())
}

fn main() -> io::Result<()> {
    run("a command that prints", "/bin/echo", &["hello from a pty"])?;
    run(
        "several lines",
        "/bin/sh",
        &["-c", "for i in 1 2 3; do echo line $i; done"],
    )?;
    run(
        "output with colour, which the emulator must not print as text",
        "/bin/sh",
        &["-c", "printf '\\033[1;32mgreen\\033[0m and plain\\n'"],
    )?;
    run(
        "a command that fails",
        "/bin/sh",
        &["-c", "echo to stdout; echo to stderr 1>&2; exit 3"],
    )?;
    Ok(())
}
