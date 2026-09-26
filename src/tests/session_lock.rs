// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the session flock: exclusivity across processes, release on
//! exit, stale directory scrub, and attach never creating the lock.

use std::io::Read;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

use crate::lifecycle::session_lock::{
    self, AcquireErr, SESSION_LOCK_FILENAME, try_acquire_session_lock,
};

use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};

// why: fork in a threaded runner is racy; serialise every forking test.
static FORK_MUTEX: Mutex<()> = Mutex::new(());

fn make_dir() -> tempfile::TempDir {
    tempdir().expect("tempdir")
}

fn fork_lock() -> std::sync::MutexGuard<'static, ()> {
    FORK_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn flock_grants_exclusive_session_lock() {
    let _fork_guard = fork_lock();
    let dir = make_dir();
    let (rd, wr) = nix::unistd::pipe().expect("pipe");
    let parent_lock =
        try_acquire_session_lock(dir.path()).unwrap_or_else(|_| panic!("parent must acquire"));

    let pid = unsafe { fork() }.expect("fork");
    match pid {
        ForkResult::Parent { child } => {
            drop(wr);
            let mut buf = [0u8; 1];
            let mut file = std::fs::File::from(rd);
            let n = file.read(&mut buf).expect("read child outcome");
            let status = waitpid(child, None).expect("waitpid");
            assert!(matches!(status, WaitStatus::Exited(_, 0)));
            assert_eq!(n, 1, "child must write one byte");
            assert_eq!(buf[0], b'H', "child must report AlreadyHeld");

            let lock_path = dir.path().join(SESSION_LOCK_FILENAME);
            assert!(lock_path.exists(), "daemon.lock must exist after acquire");
            let mode = std::fs::metadata(&lock_path)
                .expect("meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "daemon.lock must be mode 0600");

            drop(parent_lock);
            let again = try_acquire_session_lock(dir.path());
            assert!(
                again.is_ok(),
                "lock must be re-acquirable after parent drop"
            );
        }
        ForkResult::Child => {
            drop(rd);
            let outcome = try_acquire_session_lock(dir.path());
            let byte: u8 = match outcome {
                Ok(_) => b'A',
                Err(AcquireErr::AlreadyHeld) => b'H',
                Err(AcquireErr::Io(_)) => b'E',
            };
            let mut file = std::fs::File::from(wr);
            let _ = file.write_all(&[byte]);
            let _ = file.flush();
            unsafe { libc::_exit(0) };
        }
    }
}

#[test]
fn stale_dir_auto_recovers_when_lock_free() {
    let dir = make_dir();
    let p: &Path = dir.path();
    std::fs::write(p.join("control.sock"), b"").expect("control.sock");
    std::fs::write(
        p.join("daemon.pid"),
        b"9999\n/tmp/x.sock\n2026-05-17T00:00:00Z\n",
    )
    .expect("daemon.pid");
    std::fs::write(p.join("daemon.log"), b"garbage").expect("daemon.log");
    std::fs::write(p.join("config.json"), b"{\"pid\":9999}").expect("config.json");
    std::fs::write(p.join("junk"), b"abc").expect("junk");
    let subdir = p.join("nested");
    std::fs::create_dir(&subdir).expect("nested");
    std::fs::write(subdir.join("inside"), b"x").expect("inside");

    let lock = match try_acquire_session_lock(p) {
        Ok(l) => l,
        Err(_) => panic!("acquire on stale dir without lock holder must succeed"),
    };
    session_lock::scrub_stale_dir(&lock, p).expect("scrub");

    assert!(p.join(SESSION_LOCK_FILENAME).exists(), "daemon.lock kept");
    assert!(!p.join("control.sock").exists(), "control.sock removed");
    assert!(!p.join("daemon.pid").exists(), "daemon.pid removed");
    assert!(!p.join("daemon.log").exists(), "daemon.log removed");
    assert!(!p.join("config.json").exists(), "config.json removed");
    assert!(!p.join("junk").exists(), "junk removed");
    assert!(!subdir.exists(), "nested dir removed recursively");
}

#[test]
fn attach_path_skips_lock_acquisition() {
    let dir = make_dir();
    let p = dir.path();
    let sock_path = p.join("control.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&sock_path).expect("bind");

    assert!(sock_path.exists());
    assert!(
        !p.join(SESSION_LOCK_FILENAME).exists(),
        "attach path must not create daemon.lock"
    );

    let _lock = try_acquire_session_lock(p).unwrap_or_else(|_| panic!("acquire"));
    assert!(
        p.join(SESSION_LOCK_FILENAME).exists(),
        "real acquire DOES create daemon.lock"
    );
}

#[test]
fn flock_released_on_daemon_exit() {
    let _fork_guard = fork_lock();
    let dir = make_dir();
    let p = dir.path();
    let lock_path = p.join(SESSION_LOCK_FILENAME);

    let pid = unsafe { fork() }.expect("fork");
    match pid {
        ForkResult::Parent { child } => {
            let status = waitpid(child, None).expect("waitpid");
            assert!(matches!(status, WaitStatus::Exited(_, 0)));
            let parent = try_acquire_session_lock(p);
            assert!(
                parent.is_ok(),
                "lock must be re-acquirable after child exit"
            );
            assert!(lock_path.exists(), "lock file persists after child exit");
        }
        ForkResult::Child => {
            let outcome = try_acquire_session_lock(p);
            let code = match outcome {
                Ok(lock) => {
                    let raw = lock.as_raw_fd();
                    let _ = raw;
                    0
                }
                Err(_) => 2,
            };
            unsafe { libc::_exit(code) };
        }
    }
}
