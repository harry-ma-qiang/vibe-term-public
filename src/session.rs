// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The session: every pane, the focus, the layout inputs, the recorder and
//! the on-disk session directory that pane logs are written into.

use crate::grid::{GridConfig, GridLayout, SidebarSide, SplitRatio};
use crate::pane::Pane;
use crate::recorder::Recorder;
use std::path::PathBuf;

/// One multiplexer session: its panes and everything shared between them.
pub struct Session {
    pub panes: Vec<Pane>,
    pub focused: usize,
    pub recorder: Option<Recorder>,
    pub term_cols: usize,
    pub term_rows: usize,
    pub sidebar_count: usize,
    pub sidebar_side: SidebarSide,
    pub split_ratio: SplitRatio,
    pub filter_enabled: bool,
    pub hover_x: usize,
    pub hover_y: usize,
    pub session_dir: Option<PathBuf>,
    pub hovered_close: Option<usize>,
    pub hovered_copy: Option<usize>,
    pub copy_requested: Option<usize>,
    pub stage_mode: bool,
    pub name: String,
    pub needs_redraw: bool,
    pub headless: bool,
    pub detach_viewers: bool,
    pub clean_session: bool,
    pub last_click_time: std::time::Instant,
    pub last_click_x: u16,
    pub last_click_y: u16,
    pub limits: crate::monitor::ResourceLimits,
    pub restart_queue: Vec<(std::time::Instant, String)>,
    pub pending_quit_confirm: Option<std::time::Instant>,
    pub shutdown_requested: bool,
    pub selection: Option<crate::selection::Selection>,
    next_pane_id: u32,
}

impl Session {
    /// A session with no panes, sized to the host terminal.
    pub fn new(term_cols: usize, term_rows: usize) -> Self {
        Self {
            panes: Vec::new(),
            focused: 0,
            recorder: None,
            term_cols,
            term_rows,
            sidebar_count: 0,
            sidebar_side: SidebarSide::Left,
            split_ratio: SplitRatio::GoldenRatio,
            filter_enabled: false,
            hover_x: 0,
            hover_y: 0,
            session_dir: None,
            hovered_close: None,
            hovered_copy: None,
            copy_requested: None,
            stage_mode: false,
            last_click_time: std::time::Instant::now() - std::time::Duration::from_secs(10),
            last_click_x: 0,
            last_click_y: 0,
            name: String::new(),
            needs_redraw: false,
            headless: false,
            detach_viewers: false,
            clean_session: false,
            limits: crate::monitor::ResourceLimits::default(),
            restart_queue: Vec::new(),
            pending_quit_confirm: None,
            shutdown_requested: false,
            selection: None,
            next_pane_id: 0,
        }
    }

    /// Hand out the next pane id; ids are monotonic and never reused.
    pub fn assign_pane_id(&mut self) -> u32 {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        id
    }

    /// The pane with this id, if it is still open.
    pub fn pane_by_id(&self, id: u32) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == id)
    }

    /// The pane with this id, mutably, if it is still open.
    pub fn pane_by_id_mut(&mut self, id: u32) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == id)
    }

    /// The slot index of the pane with this id.
    pub fn pane_index_by_id(&self, id: u32) -> Option<usize> {
        self.panes.iter().position(|p| p.id == id)
    }

    /// Lay the grid out from the session's current geometry settings.
    pub fn compute_layout(&self) -> GridLayout {
        GridLayout::compute(&GridConfig {
            cols: self.term_cols,
            rows: self.term_rows,
            sidebar_count: self.sidebar_count,
            sidebar_side: self.sidebar_side,
            split_ratio: self.split_ratio,
        })
    }

    /// The directory holding one log file per pane.
    pub fn panes_dir(&self) -> Option<PathBuf> {
        self.session_dir.as_ref().map(|d| d.join("panes"))
    }

    /// Open one pane's log file, creating the pane directory first.
    pub fn open_pane_log(&self, pane: &mut Pane) {
        if let Some(dir) = self.panes_dir() {
            let _ = std::fs::create_dir_all(&dir);
            pane.open_log(&dir);
        }
    }

    /// Open a log file for every pane currently in the session.
    pub fn open_all_pane_logs(&mut self) {
        if let Some(dir) = self.panes_dir() {
            let _ = std::fs::create_dir_all(&dir);
            for pane in &mut self.panes {
                pane.open_log(&dir);
            }
        }
    }

    /// The single-line status bar text for this session's state.
    pub fn status_bar_text(&self) -> String {
        crate::status_bar::build_text(self.filter_enabled, self.headless)
    }
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
