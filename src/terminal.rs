// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Host terminal setup and restore, plus resize and termination signals.

use crossterm::terminal;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// False when the ui draws on the primary screen instead of the alternate one.
pub static USE_ALT_SCREEN: AtomicBool = AtomicBool::new(true);
/// Pid of the foreground child, for signal forwarding.
pub static CHILD_PID: AtomicI32 = AtomicI32::new(0);
/// Set by the `SIGWINCH` handler; cleared once the resize is applied.
pub static NEED_RESIZE: AtomicBool = AtomicBool::new(false);

/// Enter raw mode, the alternate screen, mouse capture and bracketed paste.
pub fn setup_terminal(no_alt_screen: bool) -> io::Result<()> {
    // why: raw mode fails when stdin is not a tty; escape sequences still work.
    let _ = terminal::enable_raw_mode();

    if no_alt_screen {
        USE_ALT_SCREEN.store(false, Ordering::Relaxed);
    } else {
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    }
    crossterm::execute!(io::stdout(), crossterm::event::EnableMouseCapture)?;
    // why: bracketed paste delivers one paste event instead of per-key events.
    crossterm::execute!(io::stdout(), crossterm::event::EnableBracketedPaste)?;
    Ok(())
}

/// Undo everything `setup_terminal` enabled, best effort.
pub fn restore_terminal() {
    let _ = crossterm::execute!(io::stdout(), crossterm::event::DisableBracketedPaste);
    let _ = crossterm::execute!(io::stdout(), crossterm::event::DisableMouseCapture);
    if USE_ALT_SCREEN.load(Ordering::Relaxed) {
        let mut stdout = io::stdout();
        let _ = crossterm::execute!(stdout, crossterm::terminal::LeaveAlternateScreen);
    } else {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(b"\x1b[2J\x1b[H");
        let _ = stdout.flush();
    }
    let _ = terminal::disable_raw_mode();
}

/// Restores the terminal when dropped.
pub struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Install handlers for `SIGWINCH`, `SIGTERM` and `SIGHUP`.
pub fn install_signal_handlers() {
    unsafe {
        let winch = nix::sys::signal::SigAction::new(
            nix::sys::signal::SigHandler::Handler(winch_signal),
            nix::sys::signal::SaFlags::empty(),
            nix::sys::signal::SigSet::empty(),
        );
        nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGWINCH, &winch).ok();

        let term = nix::sys::signal::SigAction::new(
            nix::sys::signal::SigHandler::Handler(term_signal),
            nix::sys::signal::SaFlags::empty(),
            nix::sys::signal::SigSet::empty(),
        );
        nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGTERM, &term).ok();
        nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGHUP, &term).ok();
    }
}

extern "C" fn winch_signal(_sig: libc::c_int) {
    NEED_RESIZE.store(true, Ordering::Relaxed);
}

extern "C" fn term_signal(sig: libc::c_int) {
    restore_terminal();
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

/// Wrap the panic hook so the terminal is restored before the message prints.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
}
