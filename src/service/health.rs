// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Health probing for a service pane: run a shell command on an interval and
//! read healthy or unhealthy out of its exit status.

use std::time::Duration;

/// A probe command with its timeout and interval, both in seconds.
/// Only the exit-status shape is supported: zero is healthy.
#[derive(Clone, Debug)]
pub struct HealthCheck {
    pub cmd: String,
    pub timeout_secs: u64,
    pub interval_secs: u64,
}

/// Result of the most recent probe, unknown until the first one finishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HealthStatus {
    #[default]
    Unknown,
    Healthy,
    Unhealthy,
}

impl HealthStatus {
    /// Coloured title-bar badge, static so painting never allocates.
    pub fn badge(self) -> &'static str {
        match self {
            Self::Unknown => "\x1b[90m[?]\x1b[0m",
            Self::Healthy => "\x1b[32m[ok]\x1b[0m",
            Self::Unhealthy => "\x1b[31m[!]\x1b[0m",
        }
    }
}

/// Run one probe: exit zero is healthy, and any other outcome is unhealthy.
/// A spawn error or a timeout both report unhealthy.
pub async fn run_probe_exit0(cmd: &str, timeout_secs: u64) -> HealthStatus {
    use tokio::process::Command;
    use tokio::time::timeout;
    let timeout_d = Duration::from_secs(timeout_secs.max(1));
    let mut child = match Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return HealthStatus::Unhealthy,
    };
    match timeout(timeout_d, child.wait()).await {
        Ok(Ok(status)) if status.success() => HealthStatus::Healthy,
        Ok(_) => HealthStatus::Unhealthy,
        Err(_) => {
            // why: kill on timeout so no grandchild lingers past the deadline.
            let _ = child.start_kill();
            HealthStatus::Unhealthy
        }
    }
}
