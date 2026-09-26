// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Main loop stall detection: the loop kicks a counter each tick, and a
//! monitor thread aborts the process when the counter stops advancing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A shared tick counter the daemon loop advances and a monitor watches.
#[derive(Clone, Default)]
pub struct Watchdog {
    counter: Arc<AtomicU64>,
}

impl Watchdog {
    /// Create a watchdog with its counter at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one main loop tick.
    pub fn kick(&self) {
        self.counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Current tick count.
    pub fn read(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// Start a thread that aborts the process after `stall_after` without a kick.
    ///
    /// It checks every `check_every` and exits once `stop` is set.
    pub fn spawn_monitor(
        self,
        stall_after: Duration,
        check_every: Duration,
        stop: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
            .name("vibe-term-watchdog".into())
            .spawn(move || {
                let mut last_count = self.read();
                let mut last_advance = Instant::now();
                loop {
                    std::thread::sleep(check_every);
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let now_count = self.read();
                    if now_count != last_count {
                        last_count = now_count;
                        last_advance = Instant::now();
                    } else if last_advance.elapsed() >= stall_after {
                        eprintln!(
                            "vibe-term daemon: main loop stalled for >= {}s — aborting for restart",
                            stall_after.as_secs()
                        );
                        std::process::abort();
                    }
                }
            })
            .expect("spawn watchdog thread")
    }
}

#[cfg(test)]
#[path = "tests/watchdog.rs"]
mod tests;
