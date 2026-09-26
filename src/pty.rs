// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Pseudoterminal spawning, io, resize and child reaping.

use nix::pty::{OpenptyResult, openpty};
use nix::sys::signal::Signal;
use nix::unistd::{ForkResult, Pid, execvp, fork, read, setsid, write};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsFd, AsRawFd, OwnedFd, RawFd};
use std::path::Path;

const EXEC_FAILURE_EXIT_CODE: i32 = 127;

/// A child process attached to the master side of a pseudoterminal.
pub struct Pty {
    pub master: OwnedFd,
    pub child_pid: Pid,
    cols: u16,
    rows: u16,
    cached_exit: Option<i32>,
}

/// Window size in character cells.
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
}

impl Pty {
    /// Spawn `cmd` on a fresh pseudoterminal of the given size.
    pub fn spawn(cmd: &str, args: &[&str], size: PtySize) -> io::Result<Self> {
        Self::spawn_with_pane_id(cmd, args, size, None)
    }

    /// Spawn `cmd`, exporting the pane id into the child environment.
    pub fn spawn_with_pane_id(
        cmd: &str,
        args: &[&str],
        size: PtySize,
        pane_id: Option<u32>,
    ) -> io::Result<Self> {
        validate_exec_cmd(cmd)?;

        let OpenptyResult { master, slave } = openpty(None, None).map_err(io::Error::other)?;

        set_window_size(master.as_raw_fd(), size.cols, size.rows)?;

        match unsafe { fork() }.map_err(io::Error::other)? {
            ForkResult::Parent { child } => {
                drop(slave);
                Ok(Pty {
                    master,
                    child_pid: child,
                    cols: size.cols,
                    rows: size.rows,
                    cached_exit: None,
                })
            }
            ForkResult::Child => {
                drop(master);
                if let Some(id) = pane_id {
                    unsafe {
                        std::env::set_var("VIBE_TERM_PANE", id.to_string());
                    }
                }
                child_setup(slave, cmd, args);
            }
        }
    }

    /// Read available output, yielding zero bytes on `EIO` or `EAGAIN`.
    pub fn read_output(&self, buf: &mut [u8]) -> io::Result<usize> {
        match read(self.master.as_fd(), buf) {
            Ok(n) => Ok(n),
            Err(nix::errno::Errno::EIO) => Ok(0),
            Err(nix::errno::Errno::EAGAIN) => Ok(0),
            Err(e) => Err(io::Error::other(e)),
        }
    }

    /// Write bytes to the pseudoterminal master.
    pub fn write_input(&self, data: &[u8]) -> io::Result<usize> {
        write(&self.master, data).map_err(io::Error::other)
    }

    /// Resize the pseudoterminal and notify the child with `SIGWINCH`.
    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        set_window_size(self.master.as_raw_fd(), cols, rows)?;
        self.cols = cols;
        self.rows = rows;
        nix::sys::signal::kill(self.child_pid, Signal::SIGWINCH).map_err(io::Error::other)
    }

    /// True while the child is still running and unreaped.
    pub fn is_alive(&self) -> bool {
        if self.cached_exit.is_some() {
            return false;
        }
        nix::sys::wait::waitpid(self.child_pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG))
            .map(|status| matches!(status, nix::sys::wait::WaitStatus::StillAlive))
            .unwrap_or(false)
    }

    /// Non-blocking exit check that caches the code once observed.
    pub fn poll_exit(&mut self) -> Option<i32> {
        if let Some(code) = self.cached_exit {
            return Some(code);
        }
        match nix::sys::wait::waitpid(self.child_pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)) {
            Ok(nix::sys::wait::WaitStatus::StillAlive) => None,
            Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => {
                self.cached_exit = Some(code);
                Some(code)
            }
            Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
                let code = 128 + sig as i32;
                self.cached_exit = Some(code);
                Some(code)
            }
            Ok(_) => {
                self.cached_exit = Some(1);
                Some(1)
            }
            Err(_) => {
                self.cached_exit = Some(1);
                Some(1)
            }
        }
    }

    /// The exit code already observed, if the child has been reaped.
    pub fn cached_exit_code(&self) -> Option<i32> {
        self.cached_exit
    }

    /// Block until the child exits and return its exit code.
    pub fn wait_exit(&self) -> io::Result<i32> {
        if let Some(code) = self.cached_exit {
            return Ok(code);
        }
        match nix::sys::wait::waitpid(self.child_pid, None) {
            Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => Ok(code),
            Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => Ok(128 + sig as i32),
            Ok(_) => Ok(1),
            Err(e) => Err(io::Error::other(e)),
        }
    }
}

// why: drop must reap; signalling alone leaves zombie shells behind.
impl Drop for Pty {
    fn drop(&mut self) {
        use nix::sys::signal::{Signal, kill};
        use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
        if self.cached_exit.is_some() {
            return;
        }
        if let Ok(WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _)) =
            waitpid(self.child_pid, Some(WaitPidFlag::WNOHANG))
        {
            return;
        }
        let pgid = self.child_pid.as_raw();
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
        let _ = kill(self.child_pid, Signal::SIGKILL);
        for _ in 0..10 {
            match waitpid(self.child_pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                _ => return,
            }
        }
    }
}

// why: the forked child must never panic, so every failure path exits 127.
fn child_setup(slave: OwnedFd, cmd: &str, args: &[&str]) -> ! {
    if setsid().is_err() {
        write_child_err(b"vibe-term: setsid failed\n");
        unsafe { libc::_exit(EXEC_FAILURE_EXIT_CODE) };
    }
    let raw_fd = slave.as_raw_fd();
    unsafe {
        libc::ioctl(raw_fd, libc::TIOCSCTTY as _, 0);
        libc::dup2(raw_fd, 0);
        libc::dup2(raw_fd, 1);
        libc::dup2(raw_fd, 2);
    }
    drop(slave);

    #[cfg(target_os = "linux")]
    {
        if nix::sys::prctl::set_pdeathsig(Signal::SIGKILL).is_err() {
            write_child_err(b"vibe-term: prctl(PR_SET_PDEATHSIG) failed\n");
        }
    }

    let c_cmd = match CString::new(cmd) {
        Ok(c) => c,
        Err(_) => {
            write_child_err(b"vibe-term: cmd contains NUL byte\n");
            unsafe { libc::_exit(EXEC_FAILURE_EXIT_CODE) };
        }
    };
    let mut c_args: Vec<CString> = Vec::with_capacity(args.len() + 1);
    c_args.push(c_cmd.clone());
    for a in args {
        match CString::new(*a) {
            Ok(c) => c_args.push(c),
            Err(_) => {
                write_child_err(b"vibe-term: arg contains NUL byte\n");
                unsafe { libc::_exit(EXEC_FAILURE_EXIT_CODE) };
            }
        }
    }
    let _ = execvp(&c_cmd, &c_args);
    write_child_err(b"vibe-term: execvp failed (command not found)\n");
    unsafe { libc::_exit(EXEC_FAILURE_EXIT_CODE) };
}

fn write_child_err(msg: &[u8]) {
    unsafe {
        libc::write(2, msg.as_ptr() as *const _, msg.len());
    }
}

/// Validate argv before forking: a path must exist, a bare name must
/// resolve through `PATH`.
pub fn validate_exec_cmd(cmd: &str) -> io::Result<()> {
    if cmd.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "exec cmd is empty",
        ));
    }
    if cmd.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "exec cmd contains NUL byte",
        ));
    }
    if cmd.contains('/') {
        let p = Path::new(cmd);
        if !p.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("exec cmd not found: {cmd}"),
            ));
        }
        return Ok(());
    }
    if find_in_path(cmd).is_some() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("exec cmd not found in PATH: {cmd}"),
        ))
    }
}

fn find_in_path(name: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn set_window_size(fd: RawFd, cols: u16, rows: u16) -> io::Result<()> {
    let ws = libc::winsize {
        ws_col: cols,
        ws_row: rows,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let ret = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/pty.rs"]
mod tests;
