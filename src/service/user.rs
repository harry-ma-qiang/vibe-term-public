// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A service pane running a user-supplied shell command: a supervisor task
//! that respawns it under policy, a probe task, and a rolling output log.

use std::collections::VecDeque;
use std::io::Write;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::runtime::Handle;
use tokio::sync::Notify;

use crate::frame::Frame;
use crate::lock::ClientId;
use crate::service::health::{HealthCheck, HealthStatus, run_probe_exit0};
use crate::service::supervisor::{CrashCounter, RestartPolicy, ServiceState, backoff_duration};
use crate::service::{InputEvent, ServicePane};

/// Lines kept per pane; a flooding service displaces old lines.
pub const MAX_SERVICE_OUTPUT_LINES: usize = 1000;

/// Seconds after a spawn during which a failure does not count as a crash.
pub const DEFAULT_START_PERIOD_SECS: u64 = 2;

/// Command, restart policy and health check for one service pane.
#[derive(Clone, Debug)]
pub struct UserServiceSpec {
    pub cmd: String,
    pub restart_policy: RestartPolicy,
    pub health: Option<HealthCheck>,
    pub start_period_secs: u64,
}

impl Default for UserServiceSpec {
    fn default() -> Self {
        Self {
            cmd: String::new(),
            restart_policy: RestartPolicy::Never,
            health: None,
            start_period_secs: DEFAULT_START_PERIOD_SECS,
        }
    }
}

struct UserServiceState {
    output: VecDeque<String>,
    state: ServiceState,
    health: HealthStatus,
    crash_counter: CrashCounter,
    last_child_pid: Option<i32>,
    cols: u16,
    rows: u16,
}

impl UserServiceState {
    fn new(policy: RestartPolicy) -> Self {
        let (max_n, win) = match policy {
            RestartPolicy::OnFailure { max_n, window_secs } => (max_n, window_secs),
            _ => (u32::MAX, 60),
        };
        Self {
            output: VecDeque::with_capacity(64),
            state: ServiceState::Starting,
            health: HealthStatus::Unknown,
            crash_counter: CrashCounter::new(max_n, win),
            last_child_pid: None,
            cols: 80,
            rows: 24,
        }
    }

    fn push_line(&mut self, line: String) {
        if self.output.len() == MAX_SERVICE_OUTPUT_LINES {
            self.output.pop_front();
        }
        self.output.push_back(line);
    }
}

static USER_SERVICE_RUNTIME: OnceLock<Handle> = OnceLock::new();

/// Install the process-wide runtime handle that service tasks are spawned on.
pub fn install_runtime(handle: Handle) {
    let _ = USER_SERVICE_RUNTIME.set(handle);
}

/// The installed process-wide runtime handle, if there is one.
pub fn runtime_handle() -> Option<Handle> {
    USER_SERVICE_RUNTIME.get().cloned()
}

static USER_ID_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// Next number for an auto-generated service pane name.
pub fn next_user_id() -> u32 {
    USER_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A service pane whose child is a shell command under a restart policy.
pub struct UserService {
    name: String,
    spec: UserServiceSpec,
    state: Arc<Mutex<UserServiceState>>,
    shutdown: Arc<Notify>,
    restart_trigger: Arc<Notify>,
}

impl UserService {
    /// Spawn on the installed runtime; Err when no runtime was installed.
    pub fn spawn(name: impl Into<String>, spec: UserServiceSpec) -> Result<Self, String> {
        let handle = runtime_handle().ok_or_else(|| {
            "no tokio runtime installed; service panes require --bridge or explicit init"
                .to_string()
        })?;
        Self::spawn_with_handle(name, spec, &handle)
    }

    /// Spawn on an explicit runtime handle rather than the installed one.
    pub fn spawn_with_handle(
        name: impl Into<String>,
        spec: UserServiceSpec,
        handle: &Handle,
    ) -> Result<Self, String> {
        let svc = Self {
            name: name.into(),
            spec: spec.clone(),
            state: Arc::new(Mutex::new(UserServiceState::new(spec.restart_policy))),
            shutdown: Arc::new(Notify::new()),
            restart_trigger: Arc::new(Notify::new()),
        };
        svc.start_supervisor(handle);
        svc.start_probe(handle);
        Ok(svc)
    }

    fn start_supervisor(&self, handle: &Handle) {
        let spec = self.spec.clone();
        let state = self.state.clone();
        let shutdown = self.shutdown.clone();
        let restart_trigger = self.restart_trigger.clone();
        handle.spawn(supervisor_loop(spec, state, shutdown, restart_trigger));
    }

    fn start_probe(&self, handle: &Handle) {
        let Some(hc) = self.spec.health.clone() else {
            return;
        };
        let state = self.state.clone();
        let shutdown = self.shutdown.clone();
        handle.spawn(probe_loop(hc, state, shutdown));
    }

    /// The current lifecycle state.
    pub fn snapshot_state(&self) -> ServiceState {
        self.state
            .lock()
            .map(|s| s.state)
            .unwrap_or(ServiceState::Starting)
    }

    /// The most recent probe result.
    pub fn snapshot_health(&self) -> HealthStatus {
        self.state
            .lock()
            .map(|s| s.health)
            .unwrap_or(HealthStatus::Unknown)
    }

    /// Pid of the running child, if one is running.
    pub fn snapshot_child_pid(&self) -> Option<i32> {
        self.state.lock().ok().and_then(|s| s.last_child_pid)
    }

    /// The retained output lines, oldest first.
    pub fn output_lines(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|s| s.output.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Set the health status directly, without running a probe.
    #[cfg(test)]
    pub fn set_health_for_test(&self, h: HealthStatus) {
        if let Ok(mut s) = self.state.lock() {
            s.health = h;
        }
    }

    /// Set the lifecycle state directly.
    #[cfg(test)]
    pub fn set_state_for_test(&self, st: ServiceState) {
        if let Ok(mut s) = self.state.lock() {
            s.state = st;
        }
    }
}

impl Drop for UserService {
    fn drop(&mut self) {
        // why: wake the supervisor so it kills the child, leaving no orphan.
        self.shutdown.notify_waiters();
    }
}

impl ServicePane for UserService {
    fn name(&self) -> &str {
        &self.name
    }

    fn handle_input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::Click { .. } => {
                if self.is_crashed() {
                    self.restart_self();
                }
            }
            InputEvent::Resize { cols, rows } => {
                if let Ok(mut s) = self.state.lock() {
                    s.cols = cols;
                    s.rows = rows;
                }
            }
            InputEvent::Bytes(_) => {}
        }
    }

    fn render(&self, out: &mut Vec<u8>, frame: &Frame) {
        let rect = frame.content_rect();
        if rect.w == 0 || rect.h == 0 {
            return;
        }
        let (state, health, lines) = match self.state.lock() {
            Ok(s) => {
                let last: Vec<String> = s
                    .output
                    .iter()
                    .rev()
                    .take(rect.h.saturating_sub(1))
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                (s.state, s.health, last)
            }
            Err(_) => (ServiceState::Starting, HealthStatus::Unknown, Vec::new()),
        };
        let header = status_line(&self.name, state, health);
        write_row(out, rect.x, rect.y, rect.w, &header);
        let log_start = rect.y + 1;
        let log_h = rect.h.saturating_sub(1);
        for i in 0..log_h {
            let row = log_start + i;
            let line = lines.get(i).cloned().unwrap_or_default();
            write_row(out, rect.x, row, rect.w, &line);
        }
    }

    fn shutdown(&mut self) {
        self.shutdown.notify_waiters();
    }

    fn accepts_input_from(&self, _observer_id: ClientId) -> bool {
        true
    }

    fn status_json(&self) -> Option<serde_json::Value> {
        let (state, health) = match self.state.lock() {
            Ok(s) => (s.state, s.health),
            Err(_) => (ServiceState::Starting, HealthStatus::Unknown),
        };
        Some(serde_json::json!({
            "state": format!("{state:?}"),
            "health": format!("{health:?}"),
            "cmd": self.spec.cmd,
        }))
    }

    fn health_badge(&self) -> &'static str {
        self.snapshot_health().badge()
    }

    fn is_crashed(&self) -> bool {
        self.snapshot_state() == ServiceState::Crashed
    }

    fn restart_self(&mut self) {
        if let Ok(mut s) = self.state.lock() {
            s.crash_counter.reset();
            s.state = ServiceState::Starting;
        }
        self.restart_trigger.notify_waiters();
    }
}

enum Wakeup {
    Shutdown,
    RespawnNow,
    BackoffElapsed,
}

enum RunOutcome {
    Shutdown,
    NeedsBackoff,
    StoppedAwaitingRestart,
    Crashed,
}

async fn supervisor_loop(
    spec: UserServiceSpec,
    state: Arc<Mutex<UserServiceState>>,
    shutdown: Arc<Notify>,
    restart_trigger: Arc<Notify>,
) {
    let mut iter: u32 = 0;
    loop {
        let last_start = Instant::now();
        set_state(&state, ServiceState::Starting);
        let outcome = match spawn_child(&spec.cmd) {
            Ok(child) => run_child(&spec, &state, &shutdown, child, last_start).await,
            Err(e) => {
                push_output(&state, format!("[spawn failed: {e}]"));
                if record_failure(&state, &spec, last_start) {
                    RunOutcome::Crashed
                } else {
                    RunOutcome::NeedsBackoff
                }
            }
        };
        let resume = match outcome {
            RunOutcome::Shutdown => return,
            RunOutcome::NeedsBackoff => wait_respawn(&shutdown, &restart_trigger, iter).await,
            RunOutcome::StoppedAwaitingRestart | RunOutcome::Crashed => {
                match wait_terminal(&shutdown, &restart_trigger).await {
                    true => return,
                    false => Wakeup::RespawnNow,
                }
            }
        };
        match resume {
            Wakeup::Shutdown => return,
            Wakeup::RespawnNow => iter = 0,
            Wakeup::BackoffElapsed => iter = iter.saturating_add(1),
        }
    }
}

fn set_state(state: &Arc<Mutex<UserServiceState>>, st: ServiceState) {
    if let Ok(mut s) = state.lock() {
        s.state = st;
    }
}

fn spawn_child(cmd: &str) -> std::io::Result<tokio::process::Child> {
    Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

async fn run_child(
    spec: &UserServiceSpec,
    state: &Arc<Mutex<UserServiceState>>,
    shutdown: &Arc<Notify>,
    mut child: tokio::process::Child,
    last_start: Instant,
) -> RunOutcome {
    if let Ok(mut s) = state.lock() {
        s.state = ServiceState::Running;
        s.last_child_pid = child.id().map(|p| p as i32);
    }
    spawn_output_reader(state.clone(), child.stdout.take());
    spawn_output_reader(state.clone(), child.stderr.take());
    let exit = tokio::select! {
        _ = shutdown.notified() => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return RunOutcome::Shutdown;
        }
        r = child.wait() => r,
    };
    if let Ok(mut s) = state.lock() {
        s.last_child_pid = None;
    }
    let success = matches!(&exit, Ok(s) if s.success());
    match decide_action(&spec.restart_policy, success) {
        Action::StopExited => {
            set_state(state, ServiceState::Exited);
            RunOutcome::StoppedAwaitingRestart
        }
        Action::Respawn => {
            set_state(state, ServiceState::Starting);
            RunOutcome::NeedsBackoff
        }
        Action::RecordFailure => {
            if record_failure(state, spec, last_start) {
                RunOutcome::Crashed
            } else {
                RunOutcome::NeedsBackoff
            }
        }
    }
}

async fn wait_terminal(shutdown: &Arc<Notify>, restart_trigger: &Arc<Notify>) -> bool {
    tokio::select! {
        _ = shutdown.notified() => true,
        _ = restart_trigger.notified() => false,
    }
}

async fn wait_respawn(shutdown: &Arc<Notify>, restart_trigger: &Arc<Notify>, iter: u32) -> Wakeup {
    tokio::select! {
        _ = shutdown.notified() => Wakeup::Shutdown,
        _ = restart_trigger.notified() => Wakeup::RespawnNow,
        _ = tokio::time::sleep(backoff_duration(iter)) => Wakeup::BackoffElapsed,
    }
}

enum Action {
    StopExited,
    Respawn,
    RecordFailure,
}

fn decide_action(policy: &RestartPolicy, success: bool) -> Action {
    match (policy, success) {
        (RestartPolicy::Never, _) => Action::StopExited,
        (RestartPolicy::Always, _) => Action::Respawn,
        (RestartPolicy::OnFailure { .. }, true) => Action::StopExited,
        (RestartPolicy::OnFailure { .. }, false) => Action::RecordFailure,
    }
}

fn record_failure(
    state: &Arc<Mutex<UserServiceState>>,
    spec: &UserServiceSpec,
    last_start: Instant,
) -> bool {
    let now = Instant::now();
    let grace = Duration::from_secs(spec.start_period_secs);
    if now.saturating_duration_since(last_start) < grace {
        // why: a failure inside the grace window must not burn the budget.
        return false;
    }
    if let Ok(mut s) = state.lock()
        && s.crash_counter.record_crash(now)
    {
        s.state = ServiceState::Crashed;
        return true;
    }
    false
}

fn spawn_output_reader<R>(state: Arc<Mutex<UserServiceState>>, reader: Option<R>)
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
{
    let Some(reader) = reader else {
        return;
    };
    tokio::spawn(async move {
        let mut buf = BufReader::new(reader).lines();
        while let Ok(Some(line)) = buf.next_line().await {
            push_output(&state, line);
        }
    });
}

fn push_output(state: &Arc<Mutex<UserServiceState>>, line: String) {
    if let Ok(mut s) = state.lock() {
        s.push_line(line);
    }
}

async fn probe_loop(hc: HealthCheck, state: Arc<Mutex<UserServiceState>>, shutdown: Arc<Notify>) {
    let interval = Duration::from_secs(hc.interval_secs.max(1));
    loop {
        tokio::select! {
            _ = shutdown.notified() => return,
            _ = tokio::time::sleep(interval) => {}
        }
        let status = run_probe_exit0(&hc.cmd, hc.timeout_secs).await;
        if let Ok(mut s) = state.lock() {
            s.health = status;
        }
    }
}

fn status_line(name: &str, state: ServiceState, health: HealthStatus) -> String {
    let st = match state {
        ServiceState::Starting => "starting",
        ServiceState::Running => "running",
        ServiceState::Exited => "exited",
        ServiceState::Crashed => "crashed",
    };
    let h = match health {
        HealthStatus::Unknown => "?",
        HealthStatus::Healthy => "ok",
        HealthStatus::Unhealthy => "!",
    };
    format!("svc:{name} [{st}] health:{h}")
}

fn write_row(out: &mut Vec<u8>, x: usize, y: usize, w: usize, text: &str) {
    let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= w {
            break;
        }
        let mut buf = [0u8; 4];
        let enc = ch.encode_utf8(&mut buf);
        out.extend_from_slice(enc.as_bytes());
        count += 1;
    }
    while count < w {
        out.push(b' ');
        count += 1;
    }
}

#[cfg(test)]
#[path = "../tests/user_service.rs"]
mod tests;
