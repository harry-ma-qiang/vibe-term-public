// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The session side of the wire verbs: sending input, reading output,
//! pattern waiting, closing panes and acquiring panes for exec and serve.

use crate::filter::SecretFilter;
use crate::frame::Frame;
use crate::pane::Pane;
use crate::session::Session;

impl Session {
    /// Write bytes to one pane's child.
    pub fn send_to_pane(&self, pane_id: u32, data: &[u8]) -> Result<(), String> {
        let pane = self
            .pane_by_id(pane_id)
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        let pty = pane.pty_opt().ok_or_else(|| {
            format!("pane {pane_id} is a service pane; send routing requires service handler")
        })?;
        pty.write_input(data).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The last lines of a pane's output, with its total line count.
    ///
    /// A pane that has gone away is recovered from the finished list instead.
    pub fn read_pane(&self, pane_id: u32, max_lines: usize) -> Result<(usize, String), String> {
        match self.pane_by_id(pane_id) {
            Some(pane) => Ok(tail_lines(&pane.capture_full_text(), max_lines)),
            None => match self.finished_text(pane_id) {
                Some(text) => Ok(tail_lines(text, max_lines)),
                None => Err(format!("pane not found: {pane_id}")),
            },
        }
    }

    /// The most recent line of a pane's output matching the pattern.
    pub fn check_pattern(&self, pane_id: u32, pattern: &str) -> Option<String> {
        let pane = self.pane_by_id(pane_id)?;
        let text = pane.capture_full_text();
        match_wait_pattern(&text, pattern, 0)
    }

    /// As `check_pattern`, but ignoring the lines before a baseline.
    pub fn check_pattern_after(
        &self,
        pane_id: u32,
        pattern: &str,
        baseline: usize,
    ) -> Option<String> {
        let pane = self.pane_by_id(pane_id)?;
        let text = pane.capture_full_text();
        match_wait_pattern(&text, pattern, baseline)
    }

    /// How many output lines a pane holds; the baseline for a wait.
    pub fn pane_line_count(&self, pane_id: u32) -> usize {
        self.pane_by_id(pane_id)
            .map(|p| p.line_count())
            .unwrap_or(0)
    }

    /// Remove a pane, kill its process group, lay the grid out again.
    pub fn close_pane(&mut self, pane_id: u32) -> Result<(), String> {
        if let Some(idx) = self.pane_index_by_id(pane_id) {
            let removed = self.panes.remove(idx);
            self.remember_finished(&removed);
            removed.kill_process_tree();
            self.sidebar_count = self.panes.len().saturating_sub(1);
            for (i, p) in self.panes.iter_mut().enumerate() {
                p.frame.slot = i;
            }
            let _ = self.relayout();
            self.needs_redraw = true;
            Ok(())
        } else {
            Err(format!("pane not found: {pane_id}"))
        }
    }

    /// Record that a managed run in this pane has finished with an exit code.
    pub fn mark_pane_completed(&mut self, pane_id: u32, exit_code: i32) {
        if let Some(pane) = self.pane_by_id_mut(pane_id) {
            pane.completed = true;
            pane.completed_exit = Some(exit_code);
            pane.exec_managed = false;
        }
    }

    /// Reuse an idle pane, or spawn a shell pane, returning its id.
    pub fn acquire_pane(&mut self) -> Result<u32, String> {
        if let Some(idle) = self
            .panes
            .iter()
            .find(|p| p.is_idle() && !p.is_completed())
            .map(|p| p.id)
        {
            return Ok(idle);
        }
        if !self.can_add_sidebar() {
            return Err("grid full, close a pane first".to_string());
        }
        self.sidebar_count += 1;
        if self.add_sidebar_pane("bash", &[]).is_err() {
            self.sidebar_count -= 1;
            return Err("spawn failed".to_string());
        }
        let id = self.last_pane_id();
        let _ = self.relayout();
        self.needs_redraw = true;
        Ok(id)
    }

    /// Spawn a fresh pane for a long-running command, returning its id.
    pub fn acquire_serve_pane(&mut self, cmd: &str) -> Result<u32, String> {
        if !self.can_add_sidebar() {
            return Err("grid full, close a pane first".to_string());
        }

        self.sidebar_count += 1;
        let layout = self.compute_layout();
        let sidebar_idx = self.panes.len() - 1;
        if sidebar_idx >= layout.sidebar_rects.len() {
            self.sidebar_count -= 1;
            return Err("grid full, close a pane first".to_string());
        }
        let rect = layout.sidebar_rects[sidebar_idx];
        let slot_idx = self.panes.len();
        let new_frame = Frame::new(rect, slot_idx, 0);
        let filter = if let Some(main) = self.panes.first() {
            main.filter.clone_secrets()
        } else {
            SecretFilter::new()
        };
        let new_id = self.assign_pane_id();
        let mut new_pane =
            Pane::spawn_with_id("bash", &["-c", cmd], new_frame, filter, Some(new_id))
                .map_err(|e| e.to_string())?;
        new_pane.id = new_id;
        new_pane.frame.pid = new_pane.pty().child_pid.as_raw();
        // why: a pane reaped before its output drains reports failure with nothing captured.
        new_pane.exec_managed = true;
        self.panes.push(new_pane);
        let _ = self.relayout();
        self.needs_redraw = true;
        Ok(new_id)
    }

    /// Reuse a completed pane, or spawn one, for a one-shot command.
    pub fn acquire_exec_pane(&mut self, cmd: &str) -> Result<u32, String> {
        if let Some(idx) = self.panes.iter().position(|p| p.is_completed()) {
            let pane = &self.panes[idx];
            let frame = pane.frame.clone();
            let filter = pane.filter.clone_secrets();
            let slot = pane.frame.slot;

            pane.kill_process_tree();

            let new_id = self.assign_pane_id();
            let mut new_pane =
                Pane::spawn_with_id("bash", &["-c", cmd], frame, filter, Some(new_id))
                    .map_err(|e| e.to_string())?;
            new_pane.id = new_id;
            new_pane.frame.slot = slot;
            new_pane.frame.pid = new_pane.pty().child_pid.as_raw();
            new_pane.exec_managed = true;
            self.panes[idx] = new_pane;
            self.needs_redraw = true;
            return Ok(new_id);
        }

        if !self.can_add_sidebar() {
            return Err("grid full, close a pane first".to_string());
        }

        self.sidebar_count += 1;
        let layout = self.compute_layout();
        let sidebar_idx = self.panes.len() - 1;
        if sidebar_idx >= layout.sidebar_rects.len() {
            self.sidebar_count -= 1;
            return Err("grid full, close a pane first".to_string());
        }
        let rect = layout.sidebar_rects[sidebar_idx];
        let slot_idx = self.panes.len();
        let new_frame = Frame::new(rect, slot_idx, 0);
        let filter = if let Some(main) = self.panes.first() {
            main.filter.clone_secrets()
        } else {
            SecretFilter::new()
        };
        let new_id = self.assign_pane_id();
        let mut new_pane =
            Pane::spawn_with_id("bash", &["-c", cmd], new_frame, filter, Some(new_id))
                .map_err(|e| e.to_string())?;
        new_pane.id = new_id;
        new_pane.frame.pid = new_pane.pty().child_pid.as_raw();
        new_pane.exec_managed = true;
        self.panes.push(new_pane);
        let _ = self.relayout();
        self.needs_redraw = true;
        Ok(new_id)
    }
}

fn tail_lines(text: &str, max_lines: usize) -> (usize, String) {
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let total = lines.len();
    let start = total.saturating_sub(max_lines);
    (total, lines[start..].join("\n"))
}

/// The most recent line after `baseline` matching any alternative.
///
/// Alternatives are split on `|`; matching is case-insensitive by design.
pub fn match_wait_pattern(text: &str, pattern: &str, baseline: usize) -> Option<String> {
    if pattern.is_empty() {
        return None;
    }
    let patterns: Vec<String> = pattern
        .split('|')
        .filter(|p| !p.is_empty())
        .map(|p| p.to_lowercase())
        .collect();
    if patterns.is_empty() {
        return None;
    }
    let new_lines: Vec<&str> = text.lines().skip(baseline).collect();
    new_lines
        .iter()
        .rev()
        .find(|l| {
            let lower = l.to_lowercase();
            patterns.iter().any(|p| lower.contains(p))
        })
        .map(|s| s.to_string())
}

#[cfg(test)]
#[path = "tests/session_api.rs"]
mod tests;
