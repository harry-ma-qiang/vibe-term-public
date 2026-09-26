// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Wall-clock helpers: current unix seconds and RFC3339 formatting by
//! civil-from-days, valid for 1970 to 9999.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the unix epoch, or zero if the clock is before it.
pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Format unix seconds as a UTC RFC3339 timestamp.
pub fn format_rfc3339(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs_today = unix_secs % 86_400;
    let h = secs_today / 3600;
    let m = (secs_today % 3600) / 60;
    let s = secs_today % 60;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
#[path = "tests/time_fmt.rs"]
mod tests;
