// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the watchdog counter and a monitor that sees steady kicks.

use super::*;

#[test]
fn watchdog_counter_advances_on_kick() {
    let w = Watchdog::new();
    assert_eq!(w.read(), 0);
    w.kick();
    assert_eq!(w.read(), 1);
    w.kick();
    w.kick();
    assert_eq!(w.read(), 3);
}

#[test]
fn watchdog_monitor_does_not_abort_on_active_kicks() {
    let stop = Arc::new(AtomicBool::new(false));
    let w = Watchdog::new();
    let _m = w.clone().spawn_monitor(
        Duration::from_millis(500),
        Duration::from_millis(50),
        Arc::clone(&stop),
    );
    for _ in 0..10 {
        w.kick();
        std::thread::sleep(Duration::from_millis(100));
    }
    stop.store(true, Ordering::Relaxed);
}
