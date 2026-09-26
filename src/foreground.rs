// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A session run in the foreground on the host terminal, which can detach
//! itself into a headless daemon.

use crate::cli::Cli;
use crate::daemon::init_session;
use crate::daemon_loop::headless_loop;
use crate::input::{InputAction, read_input};
use crate::lifecycle::signals::install_daemon_signal_handlers;
use crate::server;
use crate::session::Session;
use crate::terminal::{
    NEED_RESIZE, TerminalGuard, install_panic_hook, install_signal_handlers, setup_terminal,
};
use crossterm::terminal;
use std::io::{self, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Run a session on this terminal until every pane exits or it detaches.
pub fn run_server(mut cli: Cli) -> io::Result<()> {
    if cli.name.is_none() {
        cli.name = Some(crate::capture::generate_session_name());
    }
    let name = cli.name.clone().expect("name set above");
    let srv = server::Server::start_named(&name)?;
    unsafe {
        std::env::set_var("VIBE_TERM_SOCKET", srv.socket_path_str());
    }
    let (mut session, mut srv) = init_session(&cli, srv)?;
    setup_terminal(cli.no_alt_screen)?;
    let _guard = TerminalGuard;
    install_panic_hook();
    install_signal_handlers();

    {
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"\x1b[2J")?;
        session.render_all_borders(&mut stdout)?;
        session.render_status_bar(&mut stdout)?;
    }

    let mut last_stats = std::time::Instant::now();
    let stdin_fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(0) };

    loop {
        if NEED_RESIZE.swap(false, Ordering::Relaxed)
            && let Ok((c, r)) = terminal::size()
        {
            session.handle_resize(c as usize, r as usize)?;
            let mut stdout = io::stdout().lock();
            stdout.write_all(b"\x1b[2J")?;
            session.render_all_borders(&mut stdout)?;
            session.render_status_bar(&mut stdout)?;
            session.force_render_all(&mut stdout)?;
        }

        let pty_fds: Vec<std::os::fd::BorrowedFd> = session
            .panes
            .iter()
            // why: an ended pane's master sits in POLLHUP and would spin poll at 100% cpu.
            .filter(|p| !p.is_completed())
            .filter_map(|p| p.pty_opt().map(|pty| pty.master.as_fd()))
            .collect();
        let timeout = if last_stats.elapsed() >= Duration::from_secs(2) {
            Duration::from_millis(0)
        } else {
            Duration::from_millis(50)
        };
        let ready = crate::event_loop::poll_all(stdin_fd, srv.listener_fd(), &pty_fds, timeout);

        if ready.stdin {
            while let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
                let action = read_input();
                if matches!(action, InputAction::None) {
                    break;
                }
                let mut stdout = io::stdout().lock();
                session.handle_input(action, &mut stdout)?;
                drop(stdout);
            }
        }

        if session.detach_viewers {
            session.detach_viewers = false;
            drop(_guard);
            detach_and_daemonize(&mut session, &mut srv);
        }

        srv.poll_connections(&session);
        crate::api_dispatch::handle_api_actions(&mut srv, &mut session)?;
        srv.check_wait_clients(&session);
        crate::commands::check_run_requests(&mut srv.run_requests, &mut session);
        crate::commands::check_watch_requests(&mut srv.watch_requests, &mut session);
        crate::commands::check_notify_requests(&mut srv.notify_requests, &mut session);

        if session.needs_redraw {
            session.needs_redraw = false;
            let mut stdout = io::stdout().lock();
            stdout.write_all(b"\x1b[2J")?;
            session.render_all_borders(&mut stdout)?;
            session.render_status_bar(&mut stdout)?;
            session.force_render_all(&mut stdout)?;
        }

        if last_stats.elapsed() >= Duration::from_secs(2) {
            session.sample_stats();
            session.check_limits();
            last_stats = std::time::Instant::now();
            let mut stdout = io::stdout().lock();
            session.render_all_borders(&mut stdout)?;
        }
        session.process_restart_queue();

        {
            let mut stdout = io::stdout().lock();
            let had_output = session.poll_and_render(&mut stdout)?;
            if had_output {
                session.render_all_borders(&mut stdout)?;
            }
            session.reap_dead_panes(&mut stdout)?;
            session.position_cursor(&mut stdout)?;
        }

        if session.clear_expired_quit_confirm() {
            let mut stdout = io::stdout().lock();
            session.render_status_bar(&mut stdout)?;
        }
        if session.should_exit() || session.shutdown_requested {
            break;
        }
    }

    session.cleanup_session_dir();
    drop(srv);
    Ok(())
}

fn detach_and_daemonize(s: &mut Session, srv: &mut server::Server) -> ! {
    s.headless = true;
    s.clean_session = false;
    let _ = io::stdout().write_all(b"\x1b[2J\x1b[H");
    terminal::disable_raw_mode().ok();
    if unsafe { libc::fork() } == 0 {
        if let Ok(f) = std::fs::File::open("/dev/null") {
            unsafe {
                libc::dup2(f.as_raw_fd(), 0);
                libc::dup2(f.as_raw_fd(), 1);
                libc::dup2(f.as_raw_fd(), 2);
            }
        }
        unsafe {
            libc::setsid();
        }
        install_daemon_signal_handlers();
        let _ = headless_loop(s, srv);
    }
    std::process::exit(0);
}
