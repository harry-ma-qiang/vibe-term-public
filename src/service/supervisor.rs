// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Supervision policy for a service pane: when to respawn a child, how many
//! failures a window tolerates, and how long to wait before trying again.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// When a service pane respawns its child after the child exits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RestartPolicy {
    /// Never respawn; the pane shows exited after one run.
    #[default]
    Never,
    /// Respawn on any exit, clean or not.
    Always,
    /// Respawn until `max_n` failures inside `window_secs`, then crash.
    OnFailure { max_n: u32, window_secs: u64 },
}

/// Parse `never`, `always` or `on-failure:N:W`; None when malformed.
pub fn parse_restart_policy(s: &str) -> Option<RestartPolicy> {
    let s = s.trim();
    if s == "never" {
        return Some(RestartPolicy::Never);
    }
    if s == "always" {
        return Some(RestartPolicy::Always);
    }
    let rest = s.strip_prefix("on-failure:")?;
    let parts: Vec<&str> = rest.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let n: u32 = parts[0].parse().ok()?;
    let w: u64 = parts[1].parse().ok()?;
    if n == 0 || w == 0 {
        return None;
    }
    Some(RestartPolicy::OnFailure {
        max_n: n,
        window_secs: w,
    })
}

/// Sliding-window count of recent crashes for the on-failure policy.
#[derive(Debug)]
pub struct CrashCounter {
    timestamps: VecDeque<Instant>,
    max_n: u32,
    window_secs: u64,
}

impl CrashCounter {
    /// A counter tolerating `max_n` crashes per `window_secs` seconds.
    pub fn new(max_n: u32, window_secs: u64) -> Self {
        Self {
            timestamps: VecDeque::new(),
            max_n,
            window_secs,
        }
    }

    /// Record a crash and report whether the budget is now exhausted.
    pub fn record_crash(&mut self, now: Instant) -> bool {
        let window = Duration::from_secs(self.window_secs);
        while let Some(front) = self.timestamps.front() {
            // why: saturating, a clock that steps backwards must not panic.
            if now.saturating_duration_since(*front) > window {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }
        self.timestamps.push_back(now);
        self.timestamps.len() as u32 >= self.max_n
    }

    /// Forget every recorded crash.
    pub fn reset(&mut self) {
        self.timestamps.clear();
    }

    /// Crashes currently inside the window.
    pub fn len(&self) -> usize {
        self.timestamps.len()
    }

    /// Whether no crash is currently inside the window.
    pub fn is_empty(&self) -> bool {
        self.timestamps.is_empty()
    }
}

/// Lifecycle state of a service pane's child.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ServiceState {
    #[default]
    Starting,
    Running,
    Exited,
    Crashed,
}

/// Backoff for respawn number `iter`, doubling from 100ms up to five seconds.
pub fn backoff_duration(iter: u32) -> Duration {
    let shift = iter.min(6);
    let ms = 100u64.checked_shl(shift).unwrap_or(5000);
    Duration::from_millis(ms.min(5000))
}
