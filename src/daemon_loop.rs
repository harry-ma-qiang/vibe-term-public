// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The headless daemon loop: poll, answer requests, feed viewers, reap panes.

use crate::lifecycle::main_loop;
use crate::lifecycle::signals::DAEMON_SHUTDOWN;
use crate::server;
use crate::session::Session;
use std::io;
use std::os::fd::AsFd;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub(crate) fn headless_loop(session: &mut Session, srv: &mut server::Server) -> io::Result<()> {
    session.headless = true;
    let mut last_stats = std::time::Instant::now();
    let mut viewer_initialized = false;
    let (watchdog, _watchdog_stop) = main_loop::spawn_watchdog();

    loop {
        watchdog.kick();
        main_loop::lock_tick_stub(std::time::Instant::now());

        let pty_fds: Vec<std::os::fd::BorrowedFd> = session
            .panes
            .iter()
            // why: an ended pane's master sits in POLLHUP and would spin poll at 100% cpu.
            .filter(|p| !p.is_completed())
            .filter_map(|p| p.pty_opt().map(|pty| pty.master.as_fd()))
            .collect();
        crate::event_loop::poll_headless(srv.listener_fd(), &pty_fds, Duration::from_millis(50));

        srv.poll_connections(session);
        crate::api_dispatch::handle_api_actions(srv, session)?;
        srv.check_wait_clients(session);
        crate::commands::check_run_requests(&mut srv.run_requests, session);
        crate::commands::check_watch_requests(&mut srv.watch_requests, session);
        crate::commands::check_notify_requests(&mut srv.notify_requests, session);

        while let Some(input) = srv.recv_input() {
            if input.len() == 5 && input[0] == b'R' {
                let cols = u16::from_le_bytes([input[1], input[2]]) as usize;
                let rows = u16::from_le_bytes([input[3], input[4]]) as usize;
                if cols > 0 && rows > 0 {
                    session.handle_resize(cols, rows)?;
                    viewer_initialized = false;
                }
            } else if let Some(action) = crate::input::parse_raw_input(&input) {
                let mut buf = Vec::new();
                session.handle_input(action, &mut buf)?;
                session.position_cursor(&mut buf)?;
                if !buf.is_empty() && srv.has_display_client() {
                    srv.send_frame(&buf);
                }
            } else if let Some(pane) = session.panes.get_mut(session.focused) {
                match &mut pane.kind {
                    crate::pane::PaneKind::Pty(pty) => {
                        pty.write_input(&input)?;
                    }
                    crate::pane::PaneKind::Service { inner, .. } => {
                        inner.handle_input(crate::service::InputEvent::Bytes(input));
                    }
                }
            }
        }

        if session.detach_viewers {
            session.detach_viewers = false;
            srv.disconnect_viewers();
            viewer_initialized = false;
        }

        if srv.has_new_viewer() || !viewer_initialized && srv.has_display_client() {
            let mut buf = Vec::new();
            session.composite_frame(&mut buf)?;
            session.position_cursor(&mut buf)?;
            srv.send_frame(&buf);
            viewer_initialized = true;
        }

        if srv.has_display_client() {
            let mut buf = Vec::new();
            let had_output = session.poll_and_render(&mut buf)?;
            if had_output {
                session.render_all_borders(&mut buf)?;
                session.position_cursor(&mut buf)?;
                srv.send_frame(&buf);
            }
            if session.needs_redraw {
                session.needs_redraw = false;
                let mut buf2 = Vec::new();
                session.composite_frame(&mut buf2)?;
                session.position_cursor(&mut buf2)?;
                srv.send_frame(&buf2);
            }
        } else {
            session.poll_and_render(&mut io::sink())?;
            if session.needs_redraw {
                session.needs_redraw = false;
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(2) {
            session.sample_stats();
            last_stats = std::time::Instant::now();
            if srv.has_display_client() {
                let mut buf = Vec::new();
                session.render_all_borders(&mut buf)?;
                session.position_cursor(&mut buf)?;
                srv.send_frame(&buf);
            }
        }

        let panes_changed = session.reap_dead_panes(&mut io::sink())?;
        if panes_changed && srv.has_display_client() {
            let mut buf = Vec::new();
            session.composite_frame(&mut buf)?;
            session.position_cursor(&mut buf)?;
            srv.send_frame(&buf);
        }
        if session.clear_expired_quit_confirm() && srv.has_display_client() {
            let mut buf = Vec::new();
            session.render_status_bar(&mut buf)?;
            srv.send_frame(&buf);
        }
        if session.should_exit()
            || session.shutdown_requested
            || DAEMON_SHUTDOWN.load(Ordering::Relaxed)
        {
            break;
        }
    }
    _watchdog_stop.store(true, Ordering::Relaxed);
    Ok(())
}
