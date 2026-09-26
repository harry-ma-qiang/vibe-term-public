// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Adding, removing and relaying out a session's panes: the main pane, the
//! sidebar stack, service panes, the recorder and the chrome buttons.

use crate::filter::SecretFilter;
use crate::frame::Frame;
use crate::grid::SplitRatio;
use crate::pane::Pane;
use crate::session::Session;
use std::io;
use std::path::Path;

impl Session {
    /// Spawn the main pane, which owns the largest rectangle of the grid.
    pub fn add_main(&mut self, cmd: &str, args: &[&str], filter: SecretFilter) -> io::Result<()> {
        let layout = self.compute_layout();
        let frame = Frame::new(layout.main_rect, 0, 0);
        let id = self.assign_pane_id();
        let mut pane = Pane::spawn_with_id(cmd, args, frame, filter, Some(id))?;
        pane.id = id;
        pane.frame.pid = pane.pty().child_pid.as_raw();
        self.filter_enabled = pane.filter.is_enabled();
        self.open_pane_log(&mut pane);

        self.panes.push(pane);
        Ok(())
    }

    /// True while the terminal is tall enough for one more sidebar pane.
    pub fn can_add_sidebar(&self) -> bool {
        let current_sidebars = self.panes.len().saturating_sub(1);
        let max_sidebars = (self.term_rows.saturating_sub(1)) / 3;
        current_sidebars < max_sidebars.min(8)
    }

    /// Spawn one more sidebar pane into the next free sidebar rectangle.
    pub fn add_sidebar_pane(&mut self, cmd: &str, args: &[&str]) -> io::Result<()> {
        let layout = self.compute_layout();
        let sidebar_idx = self.panes.len() - 1;
        if sidebar_idx >= layout.sidebar_rects.len() {
            return Err(io::Error::other("grid full, close a pane first"));
        }
        let rect = layout.sidebar_rects[sidebar_idx];
        let slot = self.panes.len();
        let frame = Frame::new(rect, slot, 0);
        let filter = if let Some(main) = self.panes.first() {
            main.filter.clone_secrets()
        } else {
            SecretFilter::new()
        };
        let id = self.assign_pane_id();
        let mut pane = Pane::spawn_with_id(cmd, args, frame, filter, Some(id))?;
        pane.id = id;
        pane.frame.pid = pane.pty().child_pid.as_raw();
        self.open_pane_log(&mut pane);

        self.panes.push(pane);
        Ok(())
    }

    /// The id of the most recently added pane.
    pub fn last_pane_id(&self) -> u32 {
        self.panes.last().map(|p| p.id).unwrap_or(0)
    }

    /// Add a service pane by name, returning its new pane id.
    pub fn add_service_pane(
        &mut self,
        name: &str,
        service: Box<dyn crate::service::ServicePane>,
    ) -> io::Result<u32> {
        if !self.can_add_sidebar() {
            return Err(io::Error::other("grid full, close a pane first"));
        }
        self.sidebar_count += 1;
        let layout = self.compute_layout();
        let sidebar_idx = self.panes.len() - 1;
        if sidebar_idx >= layout.sidebar_rects.len() {
            self.sidebar_count -= 1;
            return Err(io::Error::other("grid full, close a pane first"));
        }
        let rect = layout.sidebar_rects[sidebar_idx];
        let slot_idx = self.panes.len();
        let frame = Frame::new(rect, slot_idx, 0);
        let filter = if let Some(main) = self.panes.first() {
            main.filter.clone_secrets()
        } else {
            SecretFilter::new()
        };
        let id = self.assign_pane_id();
        let mut pane = Pane::from_service(name.to_string(), service, frame, filter);
        pane.id = id;
        self.open_pane_log(&mut pane);
        self.panes.push(pane);
        let _ = self.relayout();
        self.needs_redraw = true;
        Ok(id)
    }

    /// Shut down and remove a service pane.
    pub fn kill_service_pane(&mut self, pane_id: u32) -> Result<(), String> {
        let idx = self
            .pane_index_by_id(pane_id)
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        if !self.panes[idx].is_service() {
            return Err(format!("pane {pane_id} is not a service pane"));
        }
        let mut removed = self.panes.remove(idx);
        removed.shutdown_service();
        removed.flush_log_final();
        self.sidebar_count = self.panes.len().saturating_sub(1);
        for (i, p) in self.panes.iter_mut().enumerate() {
            p.frame.slot = i;
        }
        let _ = self.relayout();
        self.needs_redraw = true;
        Ok(())
    }

    /// Replace a service pane with a fresh instance of the same service.
    pub fn restart_service_pane(&mut self, pane_id: u32) -> Result<u32, String> {
        let idx = self
            .pane_index_by_id(pane_id)
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        let name = self.panes[idx]
            .service_name()
            .ok_or_else(|| format!("pane {pane_id} is not a service pane"))?
            .to_string();
        let new_service = crate::service::create_service(&name)
            .ok_or_else(|| format!("no factory registered: {name}"))?;
        let mut old = self.panes.remove(idx);
        old.shutdown_service();
        old.flush_log_final();
        self.sidebar_count = self.panes.len().saturating_sub(1);
        for (i, p) in self.panes.iter_mut().enumerate() {
            p.frame.slot = i;
        }
        let _ = self.relayout();
        self.add_service_pane(&name, new_service)
            .map_err(|e| e.to_string())
    }

    /// Start recording the composited screen to a cast file.
    pub fn setup_recorder(&mut self, path: &Path, append: bool) -> io::Result<()> {
        if self.panes.is_empty() {
            return Ok(());
        }
        let cols = self.term_cols;
        let rows = self.term_rows;
        self.recorder = Some(if append && path.exists() {
            crate::recorder::Recorder::append(path, cols, rows)?
        } else {
            crate::recorder::Recorder::create(path, cols, rows)?
        });
        Ok(())
    }

    /// Recompute the grid and resize every pane and child to match.
    pub fn relayout(&mut self) -> io::Result<()> {
        let layout = self.compute_layout();
        if let Some(main) = self.panes.first_mut() {
            let pid = main.pty_opt().map(|p| p.child_pid.as_raw()).unwrap_or(0);
            let new_frame = Frame::new(layout.main_rect, 0, pid);
            main.resize_to_frame(new_frame)?;
        }
        let mut sidebar_idx = 0;
        for pane_idx in 1..self.panes.len() {
            if sidebar_idx < layout.sidebar_rects.len() {
                let pane = &mut self.panes[pane_idx];
                let pid = pane.pty_opt().map(|p| p.child_pid.as_raw()).unwrap_or(0);
                let new_frame = Frame::new(layout.sidebar_rects[sidebar_idx], pane_idx, pid);
                pane.resize_to_frame(new_frame)?;
                sidebar_idx += 1;
            }
        }
        if let Some(ref mut rec) = self.recorder
            && let Some(pane) = self.panes.first()
        {
            let (cc, cr) = pane.frame.child_size();
            rec.record_resize(cc, cr)?;
        }
        Ok(())
    }

    /// Adopt a new host terminal size and lay the grid out again.
    pub fn handle_resize(&mut self, cols: usize, rows: usize) -> io::Result<()> {
        self.term_cols = cols;
        self.term_rows = rows;
        self.relayout()
    }

    /// Lay out again, mark every screen dirty and save the config.
    pub fn refresh_layout(&mut self) {
        let _ = self.relayout();
        for pane in &mut self.panes {
            pane.screen.mark_all_dirty();
        }
        let _ = self.save_config();
    }

    /// How many sidebar panes currently have a live subprocess.
    pub fn sidebar_running_count(&self) -> usize {
        self.panes
            .iter()
            .skip(1)
            .filter(|p| p.has_running_child())
            .count()
    }

    /// Spawn one sidebar pane running the user's shell.
    ///
    /// The sidebar count grows first so the layout reserves a rectangle.
    pub fn add_one_pane(&mut self) -> bool {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let prev_count = self.sidebar_count;
        self.sidebar_count = prev_count + 1;
        self.refresh_layout();
        match self.add_sidebar_pane(&shell, &[]) {
            Ok(()) => {
                self.refresh_layout();
                true
            }
            Err(_) => {
                self.sidebar_count = prev_count;
                self.refresh_layout();
                false
            }
        }
    }

    /// Close every idle sidebar pane, returning the ids that were closed.
    ///
    /// The main pane is never closed this way, so the session survives.
    pub fn close_idle_panes(&mut self) -> Vec<u32> {
        let to_close: Vec<u32> = self
            .panes
            .iter()
            .enumerate()
            .filter(|(i, p)| *i != 0 && p.is_closeable_by_chrome())
            .map(|(_, p)| p.id)
            .collect();
        let mut closed = Vec::with_capacity(to_close.len());
        for id in &to_close {
            if self.close_pane(*id).is_ok() {
                closed.push(*id);
            }
        }
        closed
    }

    /// Close one pane if no subprocess is running in it; true when closed.
    pub fn close_pane_if_idle(&mut self, pane_id: u32) -> bool {
        let closeable = self
            .pane_by_id(pane_id)
            .map(|p| p.is_closeable_by_chrome())
            .unwrap_or(false);
        if !closeable {
            return false;
        }
        self.close_pane(pane_id).is_ok()
    }

    /// Grow or shrink the sidebar stack; false when the request is refused.
    pub fn set_sidebar_count(&mut self, count: usize) -> bool {
        if count == self.sidebar_count {
            return false;
        }
        let active = self.sidebar_running_count();
        if count < active {
            return false;
        }
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        if count > self.sidebar_count {
            let current_sidebar = self.panes.len() - 1;
            let needed = count.saturating_sub(current_sidebar);
            self.sidebar_count = count;
            for _ in 0..needed {
                let _ = self.add_sidebar_pane(&shell, &[]);
            }
        } else {
            let target = count + 1;
            let to_remove: Vec<usize> = (1..self.panes.len())
                .rev()
                .filter(|&i| !self.panes[i].has_running_child())
                .take(self.panes.len().saturating_sub(target))
                .collect();
            for i in to_remove {
                let removed = self.panes.remove(i);
                removed.kill_process_tree();
            }
            self.sidebar_count = self.panes.len().saturating_sub(1);
        }
        self.refresh_layout();
        true
    }

    /// Change the main-to-sidebar split; false when it is already set.
    pub fn set_split_ratio(&mut self, ratio: SplitRatio) -> bool {
        if ratio == self.split_ratio {
            return false;
        }
        self.split_ratio = ratio;
        self.refresh_layout();
        true
    }

    /// Turn secret redaction on or off across every pane at once.
    pub fn toggle_filter(&mut self) {
        self.filter_enabled = !self.filter_enabled;
        for pane in &mut self.panes {
            if self.filter_enabled {
                pane.filter.enable();
            } else {
                pane.filter.disable();
            }
        }
        self.refresh_layout();
    }
}
