// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! One pane: either a child pseudoterminal or a service, plus its screen,
//! scroll position, redaction, chrome and resource stats.

use crate::emulator::Screen;
use crate::filter::SecretFilter;
use crate::frame::Frame;
use crate::monitor::PaneStats;
use crate::pty::{Pty, PtySize};
use crate::renderer::Renderer;
use crate::service::ServicePane;
use std::io::{self, Write};
use std::os::fd::{AsFd, AsRawFd};

/// Output ceiling for one pane, past which its child is killed.
///
/// A CPU safety valve only; scrollback bounds memory independently.
pub const MAX_PANE_OUTPUT_BYTES: usize = 1024 * 1024 * 1024;

// why: exec and serve always wrap in `<shell> -c`; status shows the payload only.
fn display_cmd(cmd: &str, args: &[&str]) -> String {
    if matches!(cmd, "bash" | "zsh" | "sh") && args.len() == 2 && args[0] == "-c" {
        return args[1].to_string();
    }
    format!("{} {}", cmd, args.join(" ")).trim().to_string()
}

/// What sits behind a pane: a child pseudoterminal or a built-in service.
pub enum PaneKind {
    Pty(Pty),
    Service {
        name: String,
        inner: Box<dyn ServicePane>,
    },
}

impl PaneKind {
    /// Short wire label for this kind.
    pub fn label(&self) -> &'static str {
        match self {
            PaneKind::Pty(_) => "pty",
            PaneKind::Service { .. } => "service",
        }
    }

    /// True for a pseudoterminal pane.
    pub fn is_pty(&self) -> bool {
        matches!(self, PaneKind::Pty(_))
    }

    /// True for a service pane.
    pub fn is_service(&self) -> bool {
        matches!(self, PaneKind::Service { .. })
    }
}

/// A single pane and everything the daemon tracks about it.
pub struct Pane {
    pub id: u32,
    pub kind: PaneKind,
    pub screen: Screen,
    pub filter: SecretFilter,
    pub frame: Frame,
    pub renderer: Renderer,
    pub scroll_offset: usize,
    pub highlight: Option<crate::selection::Selection>,
    pub stats: PaneStats,
    pub original_cmd: String,
    pub completed: bool,
    pub completed_exit: Option<i32>,
    pub exec_managed: bool,
    pub total_output_bytes: usize,
    pub output_capped: bool,
}

impl Pane {
    /// True when the pane has finished or its shell sits at a prompt.
    pub fn is_idle(&self) -> bool {
        match &self.kind {
            PaneKind::Pty(_) => self.completed || (self.is_alive() && !self.has_running_child()),
            PaneKind::Service { .. } => false,
        }
    }

    /// True once a managed run has been marked finished.
    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// The pane's pseudoterminal; panics on a service pane.
    pub fn pty(&self) -> &Pty {
        match &self.kind {
            PaneKind::Pty(p) => p,
            PaneKind::Service { .. } => panic!("pty() called on service pane id={}", self.id),
        }
    }

    /// The pane's pseudoterminal, mutably; panics on a service pane.
    pub fn pty_mut(&mut self) -> &mut Pty {
        let id = self.id;
        match &mut self.kind {
            PaneKind::Pty(p) => p,
            PaneKind::Service { .. } => panic!("pty_mut() called on service pane id={id}"),
        }
    }

    /// The pane's pseudoterminal, or None for a service pane.
    pub fn pty_opt(&self) -> Option<&Pty> {
        match &self.kind {
            PaneKind::Pty(p) => Some(p),
            PaneKind::Service { .. } => None,
        }
    }

    /// The pane's pseudoterminal mutably, or None for a service pane.
    pub fn pty_opt_mut(&mut self) -> Option<&mut Pty> {
        match &mut self.kind {
            PaneKind::Pty(p) => Some(p),
            PaneKind::Service { .. } => None,
        }
    }

    /// True for a pseudoterminal pane.
    pub fn is_pty(&self) -> bool {
        self.kind.is_pty()
    }

    /// True for a service pane.
    pub fn is_service(&self) -> bool {
        self.kind.is_service()
    }

    /// The registered service name, or None for a pseudoterminal pane.
    pub fn service_name(&self) -> Option<&str> {
        match &self.kind {
            PaneKind::Service { name, .. } => Some(name.as_str()),
            PaneKind::Pty(_) => None,
        }
    }

    /// Spawn a child in a new pane sized to the given frame.
    pub fn spawn(cmd: &str, args: &[&str], frame: Frame, filter: SecretFilter) -> io::Result<Self> {
        Self::spawn_with_id(cmd, args, frame, filter, None)
    }

    /// Spawn a child, telling it which pane id it belongs to.
    pub fn spawn_with_id(
        cmd: &str,
        args: &[&str],
        frame: Frame,
        filter: SecretFilter,
        pane_id: Option<u32>,
    ) -> io::Result<Self> {
        let (cols, rows) = frame.child_size();
        let pty = Pty::spawn_with_pane_id(
            cmd,
            args,
            PtySize {
                cols: cols as u16,
                rows: rows as u16,
            },
            pane_id,
        )?;
        let screen = Screen::new(cols, rows);
        let mut renderer = Renderer::new(cols, rows);
        let content = frame.content_rect();
        renderer.set_offset(content.y, content.x);
        let original_cmd = display_cmd(cmd, args);
        Ok(Self {
            id: 0,
            kind: PaneKind::Pty(pty),
            screen,
            filter,
            frame,
            renderer,
            scroll_offset: 0,
            highlight: None,
            stats: PaneStats::default(),
            original_cmd,
            completed: false,
            completed_exit: None,
            exec_managed: false,
            total_output_bytes: 0,
            output_capped: false,
        })
    }

    /// Build a pane around an already constructed service.
    pub fn from_service(
        name: impl Into<String>,
        service: Box<dyn ServicePane>,
        frame: Frame,
        filter: SecretFilter,
    ) -> Self {
        let (cols, rows) = frame.child_size();
        let screen = Screen::new(cols, rows);
        let mut renderer = Renderer::new(cols, rows);
        let content = frame.content_rect();
        renderer.set_offset(content.y, content.x);
        let name = name.into();
        let original_cmd = format!("service:{name}");
        Self {
            id: 0,
            kind: PaneKind::Service {
                name,
                inner: service,
            },
            screen,
            filter,
            frame,
            renderer,
            scroll_offset: 0,
            highlight: None,
            stats: PaneStats::default(),
            original_cmd,
            completed: false,
            completed_exit: None,
            exec_managed: false,
            total_output_bytes: 0,
            output_capped: false,
        }
    }

    /// Adopt a new frame, resizing the screen, renderer and child.
    pub fn resize_to_frame(&mut self, new_frame: Frame) -> io::Result<()> {
        self.frame = new_frame;
        let (cols, rows) = self.frame.child_size();
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        let content = self.frame.content_rect();
        self.screen.resize(cols, rows);
        self.renderer.resize(cols, rows);
        self.renderer.set_offset(content.y, content.x);
        match &mut self.kind {
            PaneKind::Pty(p) => p.resize(cols as u16, rows as u16)?,
            PaneKind::Service { inner, .. } => {
                inner.handle_input(crate::service::InputEvent::Resize {
                    cols: cols as u16,
                    rows: rows as u16,
                })
            }
        }
        Ok(())
    }

    /// Drain pending child output into the screen; true if anything arrived.
    pub fn feed_output(&mut self) -> io::Result<bool> {
        let pty = match &mut self.kind {
            PaneKind::Pty(p) => p,
            PaneKind::Service { .. } => return Ok(false),
        };
        if self.output_capped {
            let mut buf = [0u8; 8192];
            loop {
                use nix::poll::{PollFd, PollFlags, poll};
                let mut fds = [PollFd::new(pty.master.as_fd(), PollFlags::POLLIN)];
                match poll(&mut fds, 0u8) {
                    Ok(n) if n > 0 => {}
                    _ => break,
                }
                match pty.read_output(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => continue,
                }
            }
            return Ok(false);
        }
        let mut buf = [0u8; 8192];
        let mut got_output = false;
        loop {
            use nix::poll::{PollFd, PollFlags, poll};
            let mut fds = [PollFd::new(pty.master.as_fd(), PollFlags::POLLIN)];
            match poll(&mut fds, 0u8) {
                Ok(n) if n > 0 => {}
                _ => break,
            }
            match pty.read_output(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    self.total_output_bytes = self.total_output_bytes.saturating_add(n);
                    self.screen.feed(&buf[..n]);
                    got_output = true;
                    if self.total_output_bytes >= MAX_PANE_OUTPUT_BYTES {
                        self.output_capped = true;
                        let note = format!(
                            "\r\n[vibe-term: pane output exceeded {} GiB — child killed]\r\n",
                            MAX_PANE_OUTPUT_BYTES / (1024 * 1024 * 1024)
                        );
                        self.screen.feed(note.as_bytes());
                        let _ = nix::sys::signal::kill(
                            pty.child_pid,
                            nix::sys::signal::Signal::SIGTERM,
                        );
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        Ok(got_output)
    }

    /// Advance a service pane's own clock, if this is one.
    pub fn tick_service(&mut self) {
        if let PaneKind::Service { inner, .. } = &mut self.kind {
            inner.tick();
        }
    }

    /// Append a service pane's painted output to a buffer.
    pub fn render_service(&self, out: &mut Vec<u8>) {
        if let PaneKind::Service { inner, .. } = &self.kind {
            inner.render(out, &self.frame);
        }
    }

    /// True when a service pane takes input from this observer.
    pub fn service_accepts_input_from(&self, observer_id: crate::lock::ClientId) -> bool {
        match &self.kind {
            PaneKind::Service { inner, .. } => inner.accepts_input_from(observer_id),
            PaneKind::Pty(_) => false,
        }
    }

    /// Hand one input event to a service pane.
    pub fn deliver_service_input(&mut self, ev: crate::service::InputEvent) {
        if let PaneKind::Service { inner, .. } = &mut self.kind {
            inner.handle_input(ev);
        }
    }

    /// A service pane's own status document, if it publishes one.
    pub fn service_status_json(&self) -> Option<serde_json::Value> {
        match &self.kind {
            PaneKind::Service { inner, .. } => inner.status_json(),
            PaneKind::Pty(_) => None,
        }
    }

    /// True when a service pane reports itself crashed.
    pub fn is_service_crashed(&self) -> bool {
        match &self.kind {
            PaneKind::Service { inner, .. } => inner.is_crashed(),
            PaneKind::Pty(_) => false,
        }
    }

    /// Scroll back by lines, stopping at the oldest scrollback row.
    pub fn scroll_up(&mut self, lines: usize) {
        let max = self.screen.scrollback_len();
        self.scroll_offset = (self.scroll_offset + lines).min(max);
    }

    /// Scroll forward by lines, stopping at the live screen.
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Return to the live screen and force a full repaint.
    pub fn exit_scroll(&mut self) {
        self.scroll_offset = 0;
        self.screen.mark_all_dirty();
    }

    /// True while the view sits above the live screen.
    pub fn is_scrolled(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Paint this pane's border, title, state tag and buttons.
    pub fn render_border(
        &self,
        out: &mut impl Write,
        focused: bool,
        close_hovered: bool,
        copy_hovered: bool,
    ) -> io::Result<()> {
        let mut buf = String::new();
        let state_tag = if self.completed {
            "completed"
        } else if self.is_idle() {
            "idle"
        } else {
            "active"
        };
        let (badge, close_label): (&str, &str) = match &self.kind {
            PaneKind::Service { inner, .. } => (
                inner.health_badge(),
                if inner.is_crashed() { "[r]" } else { "[x]" },
            ),
            _ => ("", "[x]"),
        };
        let sep = if badge.is_empty() { "" } else { " " };
        let title = format!(
            "#{} [{}] {badge}{sep}pid:{} {:.1}% {:.0}M",
            self.id, state_tag, self.frame.pid, self.stats.cpu_percent, self.stats.mem_mb,
        );
        self.frame.render_border_with_title(
            &mut buf,
            crate::frame::BorderOpts {
                is_focused: focused,
                close_hovered,
                copy_hovered,
                title_override: Some(&title),
                close_label,
            },
        );
        out.write_all(buf.as_bytes())
    }

    /// True when a border click may close this pane.
    ///
    /// Panes with a live subprocess, and all service panes, are protected.
    pub fn is_closeable_by_chrome(&self) -> bool {
        !self.kind.is_service() && !self.has_running_child()
    }

    /// Signal the child's process group, escalating to a kill after a pause.
    pub fn kill_process_tree(&self) {
        let pty = match &self.kind {
            PaneKind::Pty(p) => p,
            PaneKind::Service { .. } => return,
        };
        let pgid = pty.child_pid.as_raw();
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
        let pid_raw = pgid;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            unsafe {
                libc::kill(-pid_raw, libc::SIGKILL);
            }
        });
    }

    /// Ask a service pane to shut itself down.
    pub fn shutdown_service(&mut self) {
        if let PaneKind::Service { inner, .. } = &mut self.kind {
            inner.shutdown();
        }
    }

    /// True when the foreground process group differs from the pane's child.
    pub fn has_running_child(&self) -> bool {
        let pty = match &self.kind {
            PaneKind::Pty(p) => p,
            PaneKind::Service { .. } => return false,
        };
        let fd = pty.master.as_raw_fd();
        let mut pgrp: libc::pid_t = 0;
        let ret = unsafe { libc::ioctl(fd, libc::TIOCGPGRP, &mut pgrp) };
        if ret == 0 && pgrp > 0 {
            return pgrp != pty.child_pid.as_raw();
        }
        false
    }

    /// True while the pane's child is still running.
    pub fn is_alive(&self) -> bool {
        match &self.kind {
            PaneKind::Pty(p) => p.is_alive(),
            PaneKind::Service { .. } => true,
        }
    }

    /// The live screen as text, soft wraps rejoined, trailing blank rows dropped.
    pub fn capture_text(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for row in 0..self.screen.rows {
            let text = self.screen.row_text(row);
            if row > 0 && self.screen.wrapped[row - 1] {
                if let Some(last) = lines.last_mut() {
                    last.push_str(&text);
                } else {
                    lines.push(text);
                }
            } else {
                lines.push(text);
            }
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Scrollback plus the live screen as text, soft wraps rejoined.
    pub fn capture_full_text(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for i in 0..self.screen.scrollback_len() {
            if let Some(text) = self.screen.scrollback_row_text(i) {
                if i > 0 && self.screen.scrollback_is_wrapped(i - 1) {
                    if let Some(last) = lines.last_mut() {
                        last.push_str(&text);
                    } else {
                        lines.push(text);
                    }
                } else {
                    lines.push(text);
                }
            }
        }
        let sb_len = self.screen.scrollback_len();
        for row in 0..self.screen.rows {
            let text = self.screen.row_text(row);
            let continues_prev = if row == 0 {
                sb_len > 0 && self.screen.scrollback_is_wrapped(sb_len - 1)
            } else {
                self.screen.wrapped[row - 1]
            };
            if continues_prev {
                if let Some(last) = lines.last_mut() {
                    last.push_str(&text);
                } else {
                    lines.push(text);
                }
            } else {
                lines.push(text);
            }
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Logical line count of the pane's whole captured text.
    pub fn line_count(&self) -> usize {
        let text = self.capture_full_text();
        text.lines().count()
    }

    /// The text a viewer currently sees, honouring scrollback position.
    pub fn visible_text(&self) -> String {
        if self.scroll_offset == 0 {
            return self.capture_text();
        }
        let sb_len = self.screen.scrollback_len();
        let view_start = sb_len.saturating_sub(self.scroll_offset);
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for row in 0..self.screen.rows {
            let abs = view_start + row;
            let (text, wrapped) = if abs < sb_len {
                (
                    self.screen.scrollback_row_text(abs).unwrap_or_default(),
                    self.screen.scrollback_is_wrapped(abs),
                )
            } else {
                let sr = abs - sb_len;
                if sr >= self.screen.rows {
                    break;
                }
                (
                    self.screen.row_text(sr),
                    self.screen.wrapped.get(sr).copied().unwrap_or(false),
                )
            };
            if wrapped && row + 1 < self.screen.rows {
                current.push_str(&text);
            } else {
                current.push_str(text.trim_end());
                lines.push(std::mem::take(&mut current));
            }
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
}

#[cfg(test)]
#[path = "tests/pane.rs"]
mod tests;
