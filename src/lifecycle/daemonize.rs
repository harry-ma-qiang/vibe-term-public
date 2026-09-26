// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Detaching: a double fork with setsid, stdio sent to `/dev/null`, and a
//! pipe beacon the grandchild writes once its server is bound.

use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use nix::sys::stat::{Mode, umask};
use nix::unistd::{ForkResult, chdir, fork, pipe, setsid};

use crate::lifecycle::time_fmt;

/// Name of the pid file inside a session directory.
pub const DAEMON_PID_FILENAME: &str = "daemon.pid";
const READY_BEACON: &[u8] = b"READY\n";
const FAIL_BEACON: &[u8] = b"FAIL\n";

/// What the launching process learns about a started daemon.
#[derive(Debug, Clone)]
pub struct DaemonHandle {
    pub pid: u32,
    pub socket: PathBuf,
    pub start_time_unix_secs: u64,
}

/// The launching process's view: the daemon signalled ready, or it did not.
#[derive(Debug)]
pub enum DaemonStatus {
    Ready(DaemonHandle),
    BindFailed,
}

/// The grandchild's view: become the daemon, then signal through the writer.
#[derive(Debug)]
pub enum GrandchildOutcome {
    BecomeDaemon { ready_writer: ReadyWriter },
}

/// One of two values; `double_fork` returns the parent's or the grandchild's.
pub enum Either<A, B> {
    Left(A),
    Right(B),
}

/// The grandchild's end of the readiness pipe.
#[derive(Debug)]
pub struct ReadyWriter {
    fd: std::os::unix::io::OwnedFd,
}

impl ReadyWriter {
    /// Tell the waiting parent the daemon is up.
    pub fn signal_ready(self) -> io::Result<()> {
        write_beacon(self.fd.as_raw_fd(), READY_BEACON)
    }

    /// Tell the waiting parent the daemon failed to start.
    pub fn signal_fail(self) -> io::Result<()> {
        write_beacon(self.fd.as_raw_fd(), FAIL_BEACON)
    }
}

/// Double fork; the parent blocks until the grandchild signals ready or fail.
///
/// The grandchild must signal before its main loop, and chdirs to `target_cwd`, else `$HOME`, else `/`.
pub fn double_fork(
    dir: &Path,
    target_cwd: Option<&Path>,
) -> io::Result<Either<DaemonStatus, GrandchildOutcome>> {
    let (rd, wr) = pipe().map_err(io::Error::from)?;
    // why: own target_cwd before fork so no borrow crosses into the grandchild.
    let target_cwd_owned: Option<PathBuf> = target_cwd.map(|p| p.to_path_buf());
    match unsafe { fork() }.map_err(io::Error::from)? {
        ForkResult::Parent { child: _ } => {
            drop(wr);
            let status = parent_wait_for_ready(rd)?;
            Ok(Either::Left(status))
        }
        ForkResult::Child => {
            drop(rd);
            setsid().map_err(io::Error::from)?;
            match unsafe { fork() }.map_err(io::Error::from)? {
                ForkResult::Parent { .. } => {
                    std::process::exit(0);
                }
                ForkResult::Child => {
                    redirect_stdio_chdir_umask(dir, target_cwd_owned.as_deref())?;
                    Ok(Either::Right(GrandchildOutcome::BecomeDaemon {
                        ready_writer: ReadyWriter { fd: wr },
                    }))
                }
            }
        }
    }
}

fn parent_wait_for_ready(rd: std::os::unix::io::OwnedFd) -> io::Result<DaemonStatus> {
    let mut buf = [0u8; 16];
    let mut total = 0usize;
    let mut file = std::fs::File::from(rd);
    loop {
        match file.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total >= buf.len() {
                    break;
                }
                if buf[..total].contains(&b'\n') {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    let body = &buf[..total];
    if body.starts_with(READY_BEACON) || body == b"READY" {
        Ok(DaemonStatus::Ready(DaemonHandle {
            pid: 0,
            socket: PathBuf::new(),
            start_time_unix_secs: time_fmt::now_unix_secs(),
        }))
    } else {
        Ok(DaemonStatus::BindFailed)
    }
}

fn write_beacon(fd: i32, body: &[u8]) -> io::Result<()> {
    let mut written = 0;
    while written < body.len() {
        let n = unsafe {
            libc::write(
                fd,
                body.as_ptr().add(written) as *const _,
                body.len() - written,
            )
        };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if n == 0 {
            break;
        }
        written += n as usize;
    }
    Ok(())
}

fn redirect_stdio_chdir_umask(dir: &Path, target_cwd: Option<&Path>) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let null = std::fs::OpenOptions::new().write(true).open("/dev/null")?;
    let null_fd = null.as_raw_fd();
    unsafe {
        if libc::dup2(null_fd, 0) < 0 || libc::dup2(null_fd, 1) < 0 || libc::dup2(null_fd, 2) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    drop(null);
    chdir_with_fallback(target_cwd);
    umask(Mode::empty());
    Ok(())
}

fn chdir_with_fallback(target_cwd: Option<&Path>) -> PathBuf {
    if let Some(p) = target_cwd
        && chdir(p).is_ok()
    {
        return p.to_path_buf();
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if chdir(&home).is_ok() {
            return home;
        }
    }
    let _ = chdir(Path::new("/"));
    PathBuf::from("/")
}

/// Write pid, socket path and RFC3339 start time to `daemon.pid`, mode 0600.
pub fn write_pid_file(dir: &Path, handle: &DaemonHandle) -> io::Result<()> {
    let path = dir.join(DAEMON_PID_FILENAME);
    let body = format!(
        "{}\n{}\n{}\n",
        handle.pid,
        handle.socket.display(),
        time_fmt::format_rfc3339(handle.start_time_unix_secs),
    );
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    f.write_all(body.as_bytes())?;
    f.flush()
}

/// Read back the pid, socket path and start time from `daemon.pid`.
pub fn read_pid_file(dir: &Path) -> io::Result<(u32, PathBuf, String)> {
    let body = std::fs::read_to_string(dir.join(DAEMON_PID_FILENAME))?;
    let mut lines = body.lines();
    let pid: u32 = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing pid"))?
        .trim()
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad pid"))?;
    let socket = PathBuf::from(
        lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing socket"))?,
    );
    let started = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing start time"))?
        .to_string();
    Ok((pid, socket, started))
}

#[cfg(test)]
#[path = "tests/daemonize.rs"]
mod tests;
