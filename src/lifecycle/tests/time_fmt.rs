// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for RFC3339 formatting of known instants and the advancing clock.

use super::*;

#[test]
fn time_fmt_known_unix_seconds() {
    assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
    assert_eq!(format_rfc3339(86_400), "1970-01-02T00:00:00Z");
    assert_eq!(format_rfc3339(1_777_939_200), "2026-05-05T00:00:00Z");
    assert_eq!(format_rfc3339(946_684_800), "2000-01-01T00:00:00Z");
    assert_eq!(format_rfc3339(86_399), "1970-01-01T23:59:59Z");
}

#[test]
fn now_unix_secs_advances() {
    let a = now_unix_secs();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let b = now_unix_secs();
    assert!(b > a, "now_unix_secs must advance: {a} -> {b}");
}
