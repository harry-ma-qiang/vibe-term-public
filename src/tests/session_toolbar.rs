// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the quit-confirm toast: arming, confirming, dismissing and the
//! five-second window after which a tick clears it.

use crate::filter::SecretFilter;
use crate::session::Session;
use std::time::{Duration, Instant};

fn build_session() -> Session {
    let mut s = Session::new(160, 45);
    s.add_main("sleep", &["30"], SecretFilter::new())
        .expect("add main");
    s
}

#[test]
fn quit_toast_initially_inactive() {
    let s = build_session();
    assert!(!s.quit_toast_active(), "toast inactive at start");
}

#[test]
fn arm_quit_toast_activates() {
    let mut s = build_session();
    let changed = s.arm_quit_toast();
    assert!(changed);
    assert!(s.quit_toast_active(), "toast active after [Q]");
}

#[test]
fn confirm_quit_yes_sets_shutdown_flag() {
    let mut s = build_session();
    s.arm_quit_toast();
    let changed = s.confirm_quit_yes();
    assert!(changed);
    assert!(s.shutdown_requested, "shutdown_requested set");
    assert!(!s.quit_toast_active(), "toast cleared on Y");
}

#[test]
fn confirm_quit_no_clears_toast() {
    let mut s = build_session();
    s.arm_quit_toast();
    let changed = s.confirm_quit_no();
    assert!(changed);
    assert!(!s.quit_toast_active(), "toast cleared on N");
    assert!(!s.shutdown_requested, "no shutdown on N");
}

#[test]
fn quit_toast_auto_dismisses_after_5s() {
    let mut s = build_session();
    s.pending_quit_confirm = Some(Instant::now() - Duration::from_secs(6));
    assert!(!s.quit_toast_active(), "expired toast does not render");
    let changed = s.clear_expired_quit_confirm();
    assert!(changed, "tick clears expired state");
    assert!(s.pending_quit_confirm.is_none());
}

#[test]
fn clear_expired_quit_confirm_is_noop_when_unset() {
    let mut s = build_session();
    let changed = s.clear_expired_quit_confirm();
    assert!(!changed);
}

#[test]
fn clear_expired_quit_confirm_keeps_recent_toast() {
    let mut s = build_session();
    s.arm_quit_toast();
    let changed = s.clear_expired_quit_confirm();
    assert!(!changed, "recent toast survives tick");
    assert!(s.quit_toast_active());
}
