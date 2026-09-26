// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Daemon tick pacing and watchdog wiring. Each tick drains input before
//! actions, then the lock step, render, broadcast, and a watchdog kick.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::watchdog::Watchdog;

/// Target interval between daemon main loop ticks.
pub const TICK_INTERVAL: Duration = Duration::from_millis(50);
/// Time without a tick after which the watchdog aborts the daemon.
pub const STALL_AFTER: Duration = Duration::from_secs(30);
/// How often the watchdog monitor checks the tick counter.
pub const CHECK_EVERY: Duration = Duration::from_secs(10);

/// Start the watchdog monitor; returns the kicker and the flag that stops it.
pub fn spawn_watchdog() -> (Watchdog, Arc<AtomicBool>) {
    let watchdog = Watchdog::new();
    let stop = Arc::new(AtomicBool::new(false));
    let _monitor = watchdog
        .clone()
        .spawn_monitor(STALL_AFTER, CHECK_EVERY, Arc::clone(&stop));
    (watchdog, stop)
}

/// Placeholder for the per-tick lock step; does nothing yet.
pub fn lock_tick_stub(_now: Instant) {}

#[cfg(test)]
#[path = "tests/main_loop.rs"]
mod tests;
