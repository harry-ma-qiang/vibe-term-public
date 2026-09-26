// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! One `poll` over stdin, the control listener and every pane's pty, with
//! a headless variant for the detached daemon.

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use std::os::fd::BorrowedFd;
use std::time::Duration;

/// Which descriptors became readable; `pty_ready` holds indexes into the pty list.
pub struct ReadyEvents {
    pub stdin: bool,
    pub listener: bool,
    pub pty_ready: Vec<usize>,
}

/// Wait up to `timeout` for stdin, the listener or any pty to become readable.
pub fn poll_all<'a>(
    stdin_fd: BorrowedFd<'a>,
    listener_fd: BorrowedFd<'a>,
    pty_fds: &[BorrowedFd<'a>],
    timeout: Duration,
) -> ReadyEvents {
    let mut fds = Vec::with_capacity(2 + pty_fds.len());
    fds.push(PollFd::new(stdin_fd, PollFlags::POLLIN));
    fds.push(PollFd::new(listener_fd, PollFlags::POLLIN));
    for fd in pty_fds {
        fds.push(PollFd::new(*fd, PollFlags::POLLIN));
    }

    let timeout_ms = timeout.as_millis() as i32;
    let pt = if timeout_ms < 0 {
        PollTimeout::NONE
    } else {
        PollTimeout::try_from(timeout_ms as u32).unwrap_or(PollTimeout::MAX)
    };
    let _ = poll(&mut fds, pt);

    let mut result = ReadyEvents {
        stdin: false,
        listener: false,
        pty_ready: Vec::new(),
    };

    if let Some(revents) = fds[0].revents() {
        result.stdin = revents.contains(PollFlags::POLLIN);
    }
    if let Some(revents) = fds[1].revents() {
        result.listener = revents.contains(PollFlags::POLLIN);
    }
    for (i, pfd) in fds[2..].iter().enumerate() {
        if let Some(revents) = pfd.revents()
            && (revents.contains(PollFlags::POLLIN) || revents.contains(PollFlags::POLLHUP))
        {
            result.pty_ready.push(i);
        }
    }

    result
}

/// Wait up to `timeout` for the listener or any pty, without stdin.
pub fn poll_headless<'a>(
    listener_fd: BorrowedFd<'a>,
    pty_fds: &[BorrowedFd<'a>],
    timeout: Duration,
) -> ReadyEvents {
    let mut fds = Vec::with_capacity(1 + pty_fds.len());
    fds.push(PollFd::new(listener_fd, PollFlags::POLLIN));
    for fd in pty_fds {
        fds.push(PollFd::new(*fd, PollFlags::POLLIN));
    }

    let timeout_ms = timeout.as_millis() as i32;
    let pt = if timeout_ms < 0 {
        PollTimeout::NONE
    } else {
        PollTimeout::try_from(timeout_ms as u32).unwrap_or(PollTimeout::MAX)
    };
    let _ = poll(&mut fds, pt);

    let mut result = ReadyEvents {
        stdin: false,
        listener: false,
        pty_ready: Vec::new(),
    };

    if let Some(revents) = fds[0].revents() {
        result.listener = revents.contains(PollFlags::POLLIN);
    }
    for (i, pfd) in fds[1..].iter().enumerate() {
        if let Some(revents) = pfd.revents()
            && (revents.contains(PollFlags::POLLIN) || revents.contains(PollFlags::POLLHUP))
        {
            result.pty_ready.push(i);
        }
    }

    result
}
