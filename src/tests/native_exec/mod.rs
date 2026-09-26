// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for running exec and serve requests end to end on real panes, and
//! the shared helpers that start and poll them.

use super::*;
use crate::capture::{RunRequest, WatchRequest};
use crate::filter::SecretFilter;
use crate::session::Session;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::time::Duration;

mod dispatch;
mod exec;
mod heredoc;
mod pane_pool;
mod parse_api;
mod read_send_wait;
mod serve;
mod serve_shape;

pub(crate) fn setup_session() -> Session {
    let mut s = Session::new(160, 45);
    s.sidebar_count = 1;
    s.add_main("sleep", &["300"], SecretFilter::new()).unwrap();
    s
}

// why: parallel tests sharing one socket name fail to bind with AddrInUse.
fn unique_server_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!(
        "test_mod_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

// why: the server removes its socket on shutdown but not the directory holding it.
struct TmpServerDir(String);
impl Drop for TmpServerDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(format!("/tmp/vibe-term-{}", self.0));
    }
}

pub(crate) fn run_exec(
    session: &mut Session,
    cmd: &str,
    timeout_secs: u32,
    no_timeout: bool,
) -> (Vec<RunRequest>, UnixStream) {
    let (s1, s2) = UnixStream::pair().unwrap();
    s1.set_nonblocking(true).unwrap();
    s2.set_nonblocking(true).unwrap();
    // why: macOS rejects a read timeout set after the peer has closed.
    s2.set_nonblocking(false).unwrap();
    s2.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let name = unique_server_name();
    let _dir = TmpServerDir(name.clone());
    let mut srv = crate::server::Server::start_named(&name).unwrap();
    let req = start_run(cmd, timeout_secs, no_timeout, None, session, s1, &mut srv);
    let mut reqs = Vec::new();
    if let Some(r) = req {
        reqs.push(r);
    }
    (reqs, s2)
}

pub(crate) fn run_watch(
    session: &mut Session,
    cmd: &str,
    pattern: &str,
    timeout_secs: u32,
    no_timeout: bool,
) -> (Vec<WatchRequest>, UnixStream) {
    let (s1, s2) = UnixStream::pair().unwrap();
    s1.set_nonblocking(true).unwrap();
    s2.set_nonblocking(true).unwrap();
    s2.set_nonblocking(false).unwrap();
    s2.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let req = start_watch(
        crate::commands::WatchSpec {
            cmd,
            pattern,
            timeout_secs,
            no_timeout,
        },
        session,
        s1,
    );
    let mut reqs = Vec::new();
    if let Some(r) = req {
        reqs.push(r);
    }
    (reqs, s2)
}

pub(crate) fn poll_until_done(
    reqs: &mut Vec<RunRequest>,
    session: &mut Session,
    timeout: Duration,
) -> String {
    let start = std::time::Instant::now();
    while !reqs.is_empty() && start.elapsed() < timeout {
        check_run_requests(reqs, session);
        if !reqs.is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    String::new()
}

pub(crate) fn poll_watch_until_done(
    reqs: &mut Vec<WatchRequest>,
    session: &mut Session,
    timeout: Duration,
) {
    let start = std::time::Instant::now();
    while !reqs.is_empty() && start.elapsed() < timeout {
        check_watch_requests(reqs, session);
        if !reqs.is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

pub(crate) fn read_response(stream: &mut UnixStream) -> serde_json::Value {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.contains(&b'\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf);
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nraw: {text}");
    })
}
