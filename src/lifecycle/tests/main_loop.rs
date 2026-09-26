// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the watchdog spawn helper and the no-op lock step.

use super::*;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

#[test]
fn spawn_watchdog_returns_kicker_that_advances_counter() {
    let (w, stop) = spawn_watchdog();
    let before = w.read();
    w.kick();
    w.kick();
    w.kick();
    let after = w.read();
    assert_eq!(after - before, 3);
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn watchdog_does_not_abort_under_normal_tick_rate() {
    let (w, stop) = spawn_watchdog();
    for _ in 0..10 {
        w.kick();
        thread::sleep(Duration::from_millis(25));
    }
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn lock_tick_stub_is_no_op() {
    lock_tick_stub(Instant::now());
}
