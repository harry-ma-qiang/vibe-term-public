// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Input lock state machine for the multi-client model: an atomic holder on
//! the hot path, timing metadata and a grace window behind an `RwLock`.

use std::sync::RwLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Identifier of a connected client; zero means "no client".
pub type ClientId = u32;

/// Seconds a holder may stay idle before the lock is released automatically.
pub const DEFAULT_LOCK_IDLE_TIMEOUT_SECS: u64 = 1800;
/// Seconds a disconnected holder keeps its reclaim window.
pub const DEFAULT_LOCK_GRACE_SECONDS: u64 = 30;
const ENV_LOCK_IDLE_TIMEOUT: &str = "VIBE_TERM_LOCK_IDLE_TIMEOUT";
const ENV_LOCK_GRACE_SECONDS: &str = "VIBE_TERM_LOCK_GRACE_SECONDS";

/// Outcome of a take, release or force-take request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockOp {
    TakenFree {
        holder: ClientId,
    },
    AlreadyHolder,
    Conflict {
        holder: ClientId,
        idle_seconds: u64,
    },
    NotHolder,
    Released,
    Forced {
        previous: ClientId,
    },
    ForcedFromGrace {
        previous: ClientId,
    },
    InGrace {
        reclaim_holder: ClientId,
        until: Instant,
    },
}

/// A lock transition that observers must be told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockBroadcast {
    AutoReleasedIdle,
    GraceExpired,
}

/// Serialisable lock status for the wire protocol.
#[derive(Debug, Clone, Serialize)]
pub struct LockStatusJson {
    pub holder: String,
    pub since: Option<String>,
    pub idle_seconds: u64,
    pub grace_until: Option<String>,
}

/// Holder, grace window and client-id minting for one session.
pub struct LockState {
    holder: AtomicU32,
    grace_holder: AtomicU32,
    next_client_id: AtomicU32,
    taken_at: RwLock<Option<Instant>>,
    last_input_at: RwLock<Option<Instant>>,
    grace_until: RwLock<Option<Instant>>,
    idle_timeout: Duration,
    grace_seconds: Duration,
}

impl Default for LockState {
    fn default() -> Self {
        Self::new()
    }
}

impl LockState {
    /// Create a free lock, reading timeouts from the environment.
    pub fn new() -> Self {
        let idle_secs = std::env::var(ENV_LOCK_IDLE_TIMEOUT)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_LOCK_IDLE_TIMEOUT_SECS);
        let grace_secs = std::env::var(ENV_LOCK_GRACE_SECONDS)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_LOCK_GRACE_SECONDS);
        Self {
            holder: AtomicU32::new(0),
            grace_holder: AtomicU32::new(0),
            next_client_id: AtomicU32::new(0),
            taken_at: RwLock::new(None),
            last_input_at: RwLock::new(None),
            grace_until: RwLock::new(None),
            idle_timeout: Duration::from_secs(idle_secs),
            grace_seconds: Duration::from_secs(grace_secs),
        }
    }

    /// The current holder, or zero when the lock is free.
    pub fn holder(&self) -> ClientId {
        self.holder.load(Ordering::Acquire)
    }

    /// The client allowed to reclaim during the grace window.
    pub fn grace_holder(&self) -> ClientId {
        self.grace_holder.load(Ordering::Acquire)
    }

    /// Idle timeout in effect for this session.
    pub fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }

    /// Grace window length in effect for this session.
    pub fn grace_seconds(&self) -> Duration {
        self.grace_seconds
    }

    /// True while a grace window is open.
    pub fn in_grace(&self) -> bool {
        self.read_grace_until().is_some()
    }

    /// Hand out the next client id, starting at one.
    pub fn mint_transient_client_id(&self) -> ClientId {
        let prev = self.next_client_id.fetch_add(1, Ordering::Relaxed);
        prev.saturating_add(1)
    }

    /// Take the lock when free, reporting the holder on conflict.
    pub fn try_take(&self, requester: ClientId) -> LockOp {
        if requester == 0 {
            return LockOp::Conflict {
                holder: self.holder.load(Ordering::Acquire),
                idle_seconds: 0,
            };
        }
        // why: grace rejects every take; reclaim goes through try_reattach_in_grace.
        if let Some(until) = self.read_grace_until() {
            let g = self.grace_holder.load(Ordering::Acquire);
            if g != 0 {
                return LockOp::InGrace {
                    reclaim_holder: g,
                    until,
                };
            }
        }
        match self
            .holder
            .compare_exchange(0, requester, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                let now = Instant::now();
                self.write_taken_at(Some(now));
                self.write_last_input_at(Some(now));
                LockOp::TakenFree { holder: requester }
            }
            Err(current) => {
                if current == requester {
                    LockOp::AlreadyHolder
                } else {
                    let idle = self.idle_seconds_for_current();
                    LockOp::Conflict {
                        holder: current,
                        idle_seconds: idle,
                    }
                }
            }
        }
    }

    /// Release the lock, but only for the current holder.
    pub fn try_release(&self, requester: ClientId) -> LockOp {
        if requester == 0 {
            return LockOp::NotHolder;
        }
        match self
            .holder
            .compare_exchange(requester, 0, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                self.write_taken_at(None);
                self.write_last_input_at(None);
                LockOp::Released
            }
            Err(_) => LockOp::NotHolder,
        }
    }

    /// Preempt the holder, clearing any open grace window.
    pub fn force_take(&self, requester: ClientId) -> LockOp {
        if requester == 0 {
            return LockOp::NotHolder;
        }
        let was_in_grace = self.read_grace_until().is_some();
        let prev = self.holder.swap(requester, Ordering::AcqRel);
        let grace_prev = self.grace_holder.swap(0, Ordering::AcqRel);
        self.write_grace_until(None);
        let now = Instant::now();
        self.write_taken_at(Some(now));
        self.write_last_input_at(Some(now));
        if was_in_grace && prev == 0 {
            LockOp::ForcedFromGrace {
                previous: grace_prev,
            }
        } else {
            LockOp::Forced { previous: prev }
        }
    }

    /// Reset the idle timer when the holder sends input.
    pub fn record_input(&self, requester: ClientId) {
        if requester != 0 && self.holder.load(Ordering::Acquire) == requester {
            self.write_last_input_at(Some(Instant::now()));
        }
    }

    /// Expire idle holders and closed grace windows, returning what changed.
    pub fn tick(&self, now: Instant) -> Vec<LockBroadcast> {
        let mut events = Vec::new();
        let holder = self.holder.load(Ordering::Acquire);
        if holder != 0 {
            let last = self.read_last_input_at();
            if let Some(t) = last
                && now.duration_since(t) > self.idle_timeout
                && self
                    .holder
                    .compare_exchange(holder, 0, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                self.write_taken_at(None);
                self.write_last_input_at(None);
                events.push(LockBroadcast::AutoReleasedIdle);
            }
        }
        let grace_until = self.read_grace_until();
        if let Some(until) = grace_until
            && now > until
        {
            self.write_grace_until(None);
            self.grace_holder.store(0, Ordering::Release);
            events.push(LockBroadcast::GraceExpired);
        }
        events
    }

    /// Open a grace window when the holder's connection closes.
    pub fn on_holder_eof(&self, eof_client_id: ClientId) {
        if eof_client_id == 0 {
            return;
        }
        if self
            .holder
            .compare_exchange(eof_client_id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.grace_holder.store(eof_client_id, Ordering::Release);
            self.write_grace_until(Some(Instant::now() + self.grace_seconds));
        }
    }

    /// Reclaim the lock during grace; only the grace holder succeeds.
    pub fn try_reattach_in_grace(&self, requester: ClientId) -> bool {
        if requester == 0 {
            return false;
        }
        if self.read_grace_until().is_none() {
            return false;
        }
        if self.grace_holder.load(Ordering::Acquire) != requester {
            return false;
        }
        self.holder.store(requester, Ordering::Release);
        self.grace_holder.store(0, Ordering::Release);
        self.write_grace_until(None);
        let now = Instant::now();
        self.write_taken_at(Some(now));
        self.write_last_input_at(Some(now));
        true
    }

    /// Snapshot the lock for the wire protocol.
    pub fn status(&self) -> LockStatusJson {
        let h = self.holder.load(Ordering::Acquire);
        let holder = if h == 0 {
            "none".to_string()
        } else {
            format!("client_id:{h}")
        };
        let since = self.read_taken_at().map(format_duration_since);
        let idle_seconds = self.idle_seconds_for_current();
        let grace_until = self.read_grace_until().map(format_duration_until);
        LockStatusJson {
            holder,
            since,
            idle_seconds,
            grace_until,
        }
    }

    /// Seconds since the holder's last input, zero when free.
    pub fn idle_seconds_for_current(&self) -> u64 {
        match self.read_last_input_at() {
            Some(t) => Instant::now().saturating_duration_since(t).as_secs(),
            None => 0,
        }
    }

    /// Raise the id floor so restored sessions never reissue an id.
    pub fn set_next_client_id_floor(&self, floor: ClientId) {
        let cur = self.next_client_id.load(Ordering::Acquire);
        if floor > cur {
            self.next_client_id.store(floor, Ordering::Release);
        }
    }

    fn read_taken_at(&self) -> Option<Instant> {
        self.taken_at.read().ok().and_then(|g| *g)
    }

    fn read_last_input_at(&self) -> Option<Instant> {
        self.last_input_at.read().ok().and_then(|g| *g)
    }

    fn read_grace_until(&self) -> Option<Instant> {
        self.grace_until.read().ok().and_then(|g| *g)
    }

    fn write_taken_at(&self, v: Option<Instant>) {
        if let Ok(mut g) = self.taken_at.write() {
            *g = v;
        }
    }

    fn write_last_input_at(&self, v: Option<Instant>) {
        if let Ok(mut g) = self.last_input_at.write() {
            *g = v;
        }
    }

    fn write_grace_until(&self, v: Option<Instant>) {
        if let Ok(mut g) = self.grace_until.write() {
            *g = v;
        }
    }

    /// Test hook: backdate the holder's last input.
    #[cfg(test)]
    pub fn force_last_input_at(&self, v: Option<Instant>) {
        self.write_last_input_at(v);
    }

    /// Test hook: move the end of the grace window.
    #[cfg(test)]
    pub fn force_grace_until(&self, v: Option<Instant>) {
        self.write_grace_until(v);
    }
}

/// Compact lock state for the status bar: `L:free`, `L:held` or `L:grace`.
pub fn format_chrome_label(state: &LockState) -> String {
    let grace = state.grace_holder();
    if grace != 0 && state.in_grace() {
        return "L:grace".to_string();
    }
    let holder = state.holder();
    if holder == 0 {
        return "L:free".to_string();
    }
    "L:held".to_string()
}

fn format_duration_since(t: Instant) -> String {
    let secs = Instant::now().saturating_duration_since(t).as_secs();
    format!("{secs}s ago")
}

fn format_duration_until(t: Instant) -> String {
    let now_inst = Instant::now();
    let remaining = if t <= now_inst {
        Duration::ZERO
    } else {
        t.saturating_duration_since(now_inst)
    };
    let wall = SystemTime::now() + remaining;
    let secs = wall
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    crate::lifecycle::time_fmt::format_rfc3339(secs)
}

#[cfg(test)]
#[path = "tests/lock.rs"]
mod tests;
