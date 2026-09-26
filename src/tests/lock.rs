// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the lock state machine: take, release, force, grace and status.

use super::*;
use std::time::{Duration, Instant};

#[test]
fn try_take_acquires_free_lock() {
    let l = LockState::new();
    let r = l.try_take(7);
    assert!(matches!(r, LockOp::TakenFree { holder: 7 }));
    assert_eq!(l.holder(), 7);
}

#[test]
fn try_take_already_holder() {
    let l = LockState::new();
    l.try_take(7);
    let r = l.try_take(7);
    assert!(matches!(r, LockOp::AlreadyHolder));
}

#[test]
fn try_take_conflict_returns_holder() {
    let l = LockState::new();
    l.try_take(7);
    let r = l.try_take(8);
    match r {
        LockOp::Conflict { holder, .. } => assert_eq!(holder, 7),
        _ => panic!("expected Conflict, got {r:?}"),
    }
}

#[test]
fn try_take_zero_client_id_rejected() {
    let l = LockState::new();
    let r = l.try_take(0);
    assert!(matches!(r, LockOp::Conflict { holder: 0, .. }));
}

#[test]
fn try_release_clears_holder() {
    let l = LockState::new();
    l.try_take(7);
    let r = l.try_release(7);
    assert!(matches!(r, LockOp::Released));
    assert_eq!(l.holder(), 0);
}

#[test]
fn try_release_non_holder_rejected() {
    let l = LockState::new();
    l.try_take(7);
    let r = l.try_release(8);
    assert!(matches!(r, LockOp::NotHolder));
    assert_eq!(l.holder(), 7);
}

#[test]
fn try_release_zero_rejected() {
    let l = LockState::new();
    l.try_take(7);
    let r = l.try_release(0);
    assert!(matches!(r, LockOp::NotHolder));
}

#[test]
fn force_take_preempts_holder() {
    let l = LockState::new();
    l.try_take(7);
    let r = l.force_take(8);
    match r {
        LockOp::Forced { previous } => assert_eq!(previous, 7),
        _ => panic!("expected Forced, got {r:?}"),
    }
    assert_eq!(l.holder(), 8);
}

#[test]
fn holder_eof_enters_grace() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    assert_eq!(l.holder(), 0);
    assert!(l.in_grace());
    assert_eq!(l.grace_holder(), 7);
}

#[test]
fn reattach_in_grace_reclaims() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    let ok = l.try_reattach_in_grace(7);
    assert!(ok);
    assert_eq!(l.holder(), 7);
    assert!(!l.in_grace());
}

#[test]
fn reattach_in_grace_wrong_id_rejected() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    let ok = l.try_reattach_in_grace(99);
    assert!(!ok);
    assert!(l.in_grace());
}

#[test]
fn grace_force_take_clears_grace_holder() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    assert_eq!(l.grace_holder(), 7);
    let r = l.force_take(8);
    assert!(matches!(r, LockOp::ForcedFromGrace { previous: 7 }));
    assert_eq!(l.grace_holder(), 0);
    let ok = l.try_reattach_in_grace(7);
    assert!(!ok);
}

#[test]
fn try_take_during_grace_from_non_holder_rejected() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    let r = l.try_take(99);
    match r {
        LockOp::InGrace { reclaim_holder, .. } => {
            assert_eq!(reclaim_holder, 7);
        }
        _ => panic!("expected InGrace, got {r:?}"),
    }
    assert_eq!(l.holder(), 0, "holder must not be set during Grace");
}

#[test]
fn try_take_during_grace_from_grace_holder_rejected() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    let r = l.try_take(7);
    match r {
        LockOp::InGrace { reclaim_holder, .. } => {
            assert_eq!(reclaim_holder, 7);
        }
        _ => panic!("expected InGrace, got {r:?}"),
    }
    assert_eq!(l.holder(), 0);
}

#[test]
fn lock_idle_timeout_auto_releases() {
    let l = LockState::new();
    l.try_take(7);
    let now = Instant::now();
    l.force_last_input_at(Some(now - Duration::from_secs(10_000)));
    let events = l.tick(now);
    assert_eq!(l.holder(), 0);
    assert!(events.contains(&LockBroadcast::AutoReleasedIdle));
}

#[test]
fn lock_grace_expired_transitions_to_observers() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    let now = Instant::now();
    l.force_grace_until(Some(now - Duration::from_secs(1)));
    let events = l.tick(now);
    assert!(!l.in_grace());
    assert_eq!(l.grace_holder(), 0);
    assert!(events.contains(&LockBroadcast::GraceExpired));
}

#[test]
fn mint_transient_client_id_monotonic() {
    let l = LockState::new();
    let a = l.mint_transient_client_id();
    let b = l.mint_transient_client_id();
    let c = l.mint_transient_client_id();
    assert_eq!(a, 1);
    assert_eq!(b, 2);
    assert_eq!(c, 3);
}

#[test]
fn status_shape_free() {
    let l = LockState::new();
    let s = l.status();
    assert_eq!(s.holder, "none");
    assert_eq!(s.idle_seconds, 0);
    assert!(s.since.is_none());
    assert!(s.grace_until.is_none());
}

#[test]
fn status_shape_held() {
    let l = LockState::new();
    l.try_take(42);
    let s = l.status();
    assert_eq!(s.holder, "client_id:42");
    assert!(s.since.is_some());
}

#[test]
fn record_input_only_for_holder() {
    let l = LockState::new();
    l.try_take(7);
    let before = l.read_last_input_at();
    std::thread::sleep(Duration::from_millis(10));
    l.record_input(99);
    let after_other = l.read_last_input_at();
    assert_eq!(before, after_other, "non-holder must not advance timer");
    l.record_input(7);
    let after_holder = l.read_last_input_at();
    assert!(after_holder > before, "holder advances timer");
}

#[test]
fn set_next_client_id_floor_only_advances() {
    let l = LockState::new();
    let _ = l.mint_transient_client_id();
    l.set_next_client_id_floor(100);
    let id = l.mint_transient_client_id();
    assert_eq!(id, 101);
    l.set_next_client_id_floor(5);
    let id2 = l.mint_transient_client_id();
    assert_eq!(id2, 102, "floor below current must not regress");
}

#[test]
fn chrome_label_free_when_no_holder() {
    let l = LockState::new();
    assert_eq!(format_chrome_label(&l), "L:free");
}

#[test]
fn chrome_label_held_drops_client_id_suffix() {
    let l = LockState::new();
    l.try_take(7);
    assert_eq!(format_chrome_label(&l), "L:held");
}

#[test]
fn chrome_label_held_idle_drops_suffix_too() {
    let l = LockState::new();
    l.try_take(7);
    let now = Instant::now();
    l.force_last_input_at(Some(now - Duration::from_secs(120)));
    let label = format_chrome_label(&l);
    assert_eq!(
        label, "L:held",
        "an idle pane drops the idle suffix: {label}"
    );
}

#[test]
fn chrome_label_grace_drops_reclaim_holder() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    assert_eq!(format_chrome_label(&l), "L:grace");
}

#[test]
fn chrome_label_back_to_held_after_grace_force_take() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    l.force_take(9);
    assert_eq!(format_chrome_label(&l), "L:held");
}

#[test]
fn force_take_clears_grace_when_called_during_grace() {
    let l = LockState::new();
    l.try_take(7);
    l.on_holder_eof(7);
    assert!(l.in_grace());
    let r = l.force_take(8);
    assert!(matches!(r, LockOp::ForcedFromGrace { previous: 7 }));
    assert!(!l.in_grace());
    assert_eq!(l.holder(), 8);
}
