// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Daemon lifecycle: detaching, session ownership, signals, tick pacing and
//! wall-clock formatting.

/// Double fork into a detached daemon, with a readiness beacon and pid file.
pub mod daemonize;
/// Tick pacing constants and the watchdog spawn helper for the daemon loop.
pub mod main_loop;
/// The per-session `daemon.lock` flock and stale directory scrub.
pub mod session_lock;
/// Daemon SIGTERM and SIGHUP handling through a shutdown flag.
pub mod signals;
/// Unix seconds and RFC3339 formatting without a date crate.
pub mod time_fmt;
