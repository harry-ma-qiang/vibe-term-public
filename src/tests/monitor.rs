// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for per-pane resource sampling.

use super::*;

#[test]
#[cfg(target_os = "linux")]
fn sample_self_process() {
    let mut stats = PaneStats::default();
    let pid = std::process::id() as i32;
    stats.sample(pid);
    assert!(stats.mem_mb > 0.0, "should read own RSS");
    std::thread::sleep(std::time::Duration::from_millis(50));
    stats.sample(pid);
    assert!(stats.cpu_percent >= 0.0, "cpu should be non-negative");
}

#[test]
fn sample_nonexistent_pid() {
    let mut stats = PaneStats::default();
    stats.sample(999999999);
    assert_eq!(stats.mem_mb, 0.0);
    assert_eq!(stats.cpu_percent, 0.0);
}
