// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for a user-command service pane: exit-status probing, the crash
//! window, the health badge, child reaping on drop and click-to-restart.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;

use crate::service::health::{HealthCheck, HealthStatus, run_probe_exit0};
use crate::service::supervisor::ServiceState;
use crate::service::supervisor::{CrashCounter, RestartPolicy};
use crate::service::user::{DEFAULT_START_PERIOD_SECS, UserService, UserServiceSpec};
use crate::service::{InputEvent, ServicePane};

static TEST_RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn ensure_runtime() -> &'static Runtime {
    TEST_RUNTIME.get_or_init(|| Runtime::new().expect("test runtime"))
}

#[test]
fn health_check_exit0_marks_unhealthy_on_nonzero_exit() {
    let rt = ensure_runtime();
    let status = rt.block_on(run_probe_exit0("false", 5));
    assert_eq!(status, HealthStatus::Unhealthy);
}

#[test]
fn health_check_exit0_marks_healthy_on_zero_exit() {
    let rt = ensure_runtime();
    let status = rt.block_on(run_probe_exit0("true", 5));
    assert_eq!(status, HealthStatus::Healthy);
}

#[test]
fn health_check_timeout_marks_unhealthy() {
    let rt = ensure_runtime();
    let status = rt.block_on(run_probe_exit0("sleep 5", 1));
    assert_eq!(status, HealthStatus::Unhealthy);
}

#[test]
fn crash_counter_window_evicts_old_entries() {
    let mut c = CrashCounter::new(3, 10);
    let base = Instant::now();
    let _ = c.record_crash(base);
    let _ = c.record_crash(base + Duration::from_secs(1));
    assert_eq!(c.len(), 2);
    let _ = c.record_crash(base + Duration::from_secs(15));
    assert_eq!(c.len(), 1, "old entries beyond window must evict");
}

#[test]
fn crash_counter_max_n_triggers_crashed_state() {
    let mut c = CrashCounter::new(3, 60);
    let base = Instant::now();
    assert!(!c.record_crash(base));
    assert!(!c.record_crash(base + Duration::from_secs(1)));
    assert!(
        c.record_crash(base + Duration::from_secs(2)),
        "third crash inside window must signal Crashed"
    );
}

#[test]
fn user_service_pane_renders_health_badge() {
    let rt = ensure_runtime();
    let spec = UserServiceSpec {
        cmd: "sleep 30".to_string(),
        restart_policy: RestartPolicy::Never,
        health: None,
        start_period_secs: DEFAULT_START_PERIOD_SECS,
    };
    let svc = UserService::spawn_with_handle("test", spec, &rt.handle().clone())
        .expect("spawn user service");
    svc.set_health_for_test(HealthStatus::Unhealthy);
    let badge = svc.health_badge();
    assert!(
        badge.contains("[!]"),
        "Unhealthy badge must contain [!] marker, got {badge:?}"
    );
    drop(svc);
}

#[test]
fn user_service_drop_cancels_probe_and_kills_child() {
    let rt = ensure_runtime();
    let spec = UserServiceSpec {
        cmd: "sleep 60".to_string(),
        restart_policy: RestartPolicy::Never,
        health: Some(HealthCheck {
            cmd: "sleep 60".to_string(),
            timeout_secs: 5,
            interval_secs: 1,
        }),
        start_period_secs: DEFAULT_START_PERIOD_SECS,
    };
    let svc = UserService::spawn_with_handle("dropme", spec, &rt.handle().clone()).expect("spawn");
    let pid = rt.block_on(async {
        for _ in 0..50 {
            if let Some(p) = svc.snapshot_child_pid() {
                return Some(p);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        None
    });
    let pid = pid.expect("child pid captured");
    drop(svc);
    let gone = rt.block_on(async {
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let ret = unsafe { libc::kill(pid, 0) };
            if ret != 0 {
                let err = std::io::Error::last_os_error()
                    .raw_os_error()
                    .unwrap_or(libc::ESRCH);
                if err == libc::ESRCH {
                    return true;
                }
            }
        }
        false
    });
    assert!(gone, "child pid {pid} must be reaped after Drop");
}

#[test]
fn user_service_click_when_crashed_triggers_restart() {
    let rt = ensure_runtime();
    let spec = UserServiceSpec {
        cmd: "sleep 30".to_string(),
        restart_policy: RestartPolicy::OnFailure {
            max_n: 1,
            window_secs: 60,
        },
        health: None,
        start_period_secs: DEFAULT_START_PERIOD_SECS,
    };
    let mut svc = UserService::spawn_with_handle("clickme", spec, &rt.handle().clone())
        .expect("spawn user service");
    svc.set_state_for_test(ServiceState::Crashed);
    assert!(svc.is_crashed(), "test setup: pane must be Crashed");
    svc.handle_input(InputEvent::Click { x: 0, y: 0 });
    assert_ne!(
        svc.snapshot_state(),
        ServiceState::Crashed,
        "Click on Crashed pane must drive state out of Crashed (restart_self)"
    );
    drop(svc);
}
