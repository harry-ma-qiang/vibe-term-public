// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Session ownership: an exclusive flock on `daemon.lock` marks the live
//! daemon, and its holder may scrub a dead daemon's leftovers.

use std::ffi::CString;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;

/// Name of the lock file inside a session directory.
pub const SESSION_LOCK_FILENAME: &str = "daemon.lock";

/// An flocked `daemon.lock` descriptor; dropping it releases the lock.
pub struct SessionLock {
    fd: OwnedFd,
}

impl SessionLock {
    /// The raw descriptor holding the lock.
    pub fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.fd.as_raw_fd()
    }
}

/// Why a session lock could not be taken.
pub enum AcquireErr {
    AlreadyHeld,
    Io(io::Error),
}

impl From<io::Error> for AcquireErr {
    fn from(e: io::Error) -> Self {
        AcquireErr::Io(e)
    }
}

/// Take a non-blocking exclusive flock on `daemon.lock` in an existing directory.
///
/// `AlreadyHeld` means another daemon owns the session.
pub fn try_acquire_session_lock(session_dir: &Path) -> Result<SessionLock, AcquireErr> {
    let lock_path = session_dir.join(SESSION_LOCK_FILENAME);
    let cpath = path_to_cstring(&lock_path)?;
    // why: O_CLOEXEC keeps pty children from inheriting and pinning the lock.
    let raw = unsafe {
        libc::open(
            cpath.as_ptr(),
            libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC,
            0o600,
        )
    };
    if raw < 0 {
        return Err(AcquireErr::Io(io::Error::last_os_error()));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let rc = unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        let _ = write_pid_body(&fd);
        Ok(SessionLock { fd })
    } else {
        let err = io::Error::last_os_error();
        // why: EWOULDBLOCK and EAGAIN may differ outside Linux; match both forms.
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) || err.kind() == io::ErrorKind::WouldBlock
        {
            Err(AcquireErr::AlreadyHeld)
        } else {
            Err(AcquireErr::Io(err))
        }
    }
}

/// Remove everything in the session directory except `daemon.lock` itself.
///
/// Failures on single entries are skipped, not returned.
pub fn scrub_stale_dir(_lock: &SessionLock, session_dir: &Path) -> io::Result<()> {
    let entries = std::fs::read_dir(session_dir)?;
    for entry in entries.flatten() {
        if entry.file_name() == SESSION_LOCK_FILENAME {
            continue;
        }
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => {
                continue;
            }
        };
        let result = if file_type.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        let _ = result;
    }
    Ok(())
}

fn path_to_cstring(p: &Path) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(p.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "lock path has interior NUL"))
}

fn write_pid_body(fd: &OwnedFd) -> io::Result<()> {
    let raw = fd.as_raw_fd();
    if unsafe { libc::ftruncate(raw, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::lseek(raw, 0, libc::SEEK_SET) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let body = format!("{}\n", std::process::id());
    let mut file = unsafe { std::fs::File::from_raw_fd(libc::dup(raw)) };
    let res = file.write_all(body.as_bytes()).and_then(|()| file.flush());
    drop(file);
    res
}

#[cfg(test)]
#[path = "../tests/session_lock.rs"]
mod tests;
