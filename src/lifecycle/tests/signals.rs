// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests that a raised SIGTERM flips the daemon shutdown flag.

use super::*;
use std::sync::Mutex;
use std::time::Duration;

static SERIALIZE: Mutex<()> = Mutex::new(());

#[test]
fn sigterm_flips_daemon_shutdown_flag() {
    let _g = SERIALIZE.lock().unwrap_or_else(|e| e.into_inner());
    reset_shutdown_for_test();
    install_daemon_signal_handlers();
    nix::sys::signal::raise(nix::sys::signal::Signal::SIGTERM).expect("raise");
    for _ in 0..40 {
        if was_shutdown_requested() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        was_shutdown_requested(),
        "DAEMON_SHUTDOWN must flip after SIGTERM"
    );
    reset_shutdown_for_test();
}
