// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for stream id allocation and cancellation: monotonicity, unknown
//! ids, token flipping, repeated cancels and cancel after release.

use super::*;

#[test]
fn id_alloc_is_monotonic() {
    let reg = StreamRegistry::new();
    let (id1, _) = reg.allocate();
    let (id2, _) = reg.allocate();
    let (id3, _) = reg.allocate();
    assert!(id2 > id1);
    assert!(id3 > id2);
}

#[test]
fn cancel_unknown_returns_false() {
    let reg = StreamRegistry::new();
    assert!(!reg.cancel(9999));
}

#[test]
fn cancel_flips_token() {
    let reg = StreamRegistry::new();
    let (id, token) = reg.allocate();
    assert!(!token.is_cancelled());
    assert!(reg.cancel(id));
    assert!(token.is_cancelled());
}

#[test]
fn second_cancel_same_id_idempotent() {
    let reg = StreamRegistry::new();
    let (id, _token) = reg.allocate();
    assert!(reg.cancel(id));
    assert!(reg.cancel(id));
}

#[test]
fn release_then_cancel_no_op() {
    let reg = StreamRegistry::new();
    let (id, _token) = reg.allocate();
    reg.release(id);
    assert!(!reg.cancel(id));
}
