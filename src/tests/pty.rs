// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for pseudoterminal spawning, io, resize and argv validation.

use super::*;
use std::thread;
use std::time::Duration;

#[test]
fn spawn_and_read_output() {
    let pty =
        Pty::spawn("echo", &["hello_pty"], PtySize { cols: 80, rows: 24 }).expect("spawn echo");
    thread::sleep(Duration::from_millis(100));
    let mut buf = [0u8; 1024];
    let n = pty.read_output(&mut buf).expect("read");
    let output = String::from_utf8_lossy(&buf[..n]);
    assert!(output.contains("hello_pty"), "got: {output}");
}

#[test]
fn write_input_to_child() {
    let pty = Pty::spawn("cat", &[], PtySize { cols: 80, rows: 24 }).expect("spawn cat");
    pty.write_input(b"ping\n").expect("write");
    thread::sleep(Duration::from_millis(100));
    let mut buf = [0u8; 1024];
    let n = pty.read_output(&mut buf).expect("read");
    let output = String::from_utf8_lossy(&buf[..n]);
    assert!(output.contains("ping"), "got: {output}");
}

#[test]
fn wait_exit_code() {
    let pty =
        Pty::spawn("sh", &["-c", "exit 42"], PtySize { cols: 80, rows: 24 }).expect("spawn sh");
    let code = pty.wait_exit().expect("wait");
    assert_eq!(code, 42);
}

#[test]
fn resize_pty() {
    let mut pty =
        Pty::spawn("sleep", &["10"], PtySize { cols: 80, rows: 24 }).expect("spawn sleep");
    assert!(pty.resize(120, 40).is_ok());
    assert!(pty.is_alive());
    nix::sys::signal::kill(pty.child_pid, Signal::SIGTERM).ok();
    pty.wait_exit().ok();
}

#[test]
fn spawn_empty_cmd_returns_err_not_panic() {
    let result = Pty::spawn("", &[], PtySize { cols: 80, rows: 24 });
    assert!(result.is_err(), "empty cmd must return Err, got Ok");
    let err = result.err().expect("err");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn spawn_nonexistent_absolute_path_returns_err() {
    let result = Pty::spawn(
        "/nonexistent/binary/that/should/not/exist",
        &[],
        PtySize { cols: 80, rows: 24 },
    );
    assert!(result.is_err(), "missing binary must return Err, got Ok");
    let err = result.err().expect("err");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn spawn_unresolvable_bare_name_returns_err() {
    let result = Pty::spawn(
        "vibe-term-no-such-binary-12345",
        &[],
        PtySize { cols: 80, rows: 24 },
    );
    assert!(
        result.is_err(),
        "unresolvable bare name must return Err, got Ok"
    );
    let err = result.err().expect("err");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn validate_exec_cmd_accepts_real_binaries() {
    super::validate_exec_cmd("sh").expect("sh in PATH");
    super::validate_exec_cmd("/bin/sh").expect("/bin/sh exists");
}

#[test]
fn validate_exec_cmd_rejects_nul_byte() {
    let err = super::validate_exec_cmd("bash\0").expect_err("nul rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[cfg(target_os = "linux")]
#[test]
fn pdeathsig_set_on_spawned_pty_child() {
    use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
    use nix::unistd::{ForkResult, Pid, fork};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Instant;

    // why: kill the intermediate parent, then confirm the kernel reaps the child.
    let (mut parent_sock, mut daemon_sock) = UnixStream::pair().expect("socketpair");

    match unsafe { fork() }.expect("fork daemon") {
        ForkResult::Parent { child: daemon_pid } => {
            drop(daemon_sock);
            let mut pid_bytes = [0u8; 4];
            parent_sock
                .read_exact(&mut pid_bytes)
                .expect("read pty pid");
            let pty_pid = Pid::from_raw(i32::from_le_bytes(pid_bytes));

            nix::sys::signal::kill(daemon_pid, Signal::SIGKILL).expect("kill daemon");
            waitpid(daemon_pid, None).expect("reap daemon");

            let deadline = Instant::now() + Duration::from_secs(3);
            let mut dead = false;
            while Instant::now() < deadline {
                match nix::sys::signal::kill(pty_pid, None) {
                    Err(nix::errno::Errno::ESRCH) => {
                        dead = true;
                        break;
                    }
                    _ => thread::sleep(Duration::from_millis(50)),
                }
            }
            if !dead {
                let _ = nix::sys::signal::kill(pty_pid, Signal::SIGKILL);
            }
            assert!(
                dead,
                "PTY child {pty_pid} should be reaped via PDEATHSIG after daemon dies",
            );
        }
        ForkResult::Child => {
            drop(parent_sock);
            let pty = match Pty::spawn("sleep", &["30"], PtySize { cols: 80, rows: 24 }) {
                Ok(p) => p,
                Err(_) => unsafe { libc::_exit(1) },
            };
            let pid_bytes = pty.child_pid.as_raw().to_le_bytes();
            if daemon_sock.write_all(&pid_bytes).is_err() {
                unsafe { libc::_exit(2) };
            }
            // why: keep the pty's parent alive until the outer test kills us.
            loop {
                thread::sleep(Duration::from_secs(10));
            }
        }
    }

    let _ = waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG))
        .map(|s| matches!(s, WaitStatus::Exited(_, _)));
}
