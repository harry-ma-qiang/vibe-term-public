// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Daemon signal handling: SIGTERM and SIGHUP flip a shutdown flag the main
//! loop polls, so it exits cleanly and removes its socket.

use std::sync::atomic::{AtomicBool, Ordering};

/// Set when the daemon has received SIGTERM or SIGHUP.
pub static DAEMON_SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Install SIGTERM and SIGHUP handlers that request a clean daemon shutdown.
pub fn install_daemon_signal_handlers() {
    // why: default termination skips Drop, which would leak the socket file.
    unsafe {
        let h = nix::sys::signal::SigAction::new(
            nix::sys::signal::SigHandler::Handler(daemon_term_signal),
            nix::sys::signal::SaFlags::empty(),
            nix::sys::signal::SigSet::empty(),
        );
        nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGTERM, &h).ok();
        nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGHUP, &h).ok();
    }
}

/// Whether a termination signal has asked the daemon to stop.
pub fn was_shutdown_requested() -> bool {
    DAEMON_SHUTDOWN.load(Ordering::Acquire)
}

/// Clear the shutdown flag between tests.
#[cfg(test)]
pub fn reset_shutdown_for_test() {
    DAEMON_SHUTDOWN.store(false, Ordering::Release);
}

extern "C" fn daemon_term_signal(_sig: libc::c_int) {
    DAEMON_SHUTDOWN.store(true, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "tests/signals.rs"]
mod tests;
