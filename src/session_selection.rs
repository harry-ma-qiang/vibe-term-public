// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A viewer's mouse selection within one pane, and copying it or a whole
//! pane to the host clipboard.

use crate::frame::HitZone;
use crate::selection::{self, Selection};
use crate::session::Session;
use std::io::Write;

impl Session {
    fn content_pane_at(&self, x: usize, y: usize) -> Option<usize> {
        self.panes
            .iter()
            .position(|p| matches!(p.frame.hit_test(x, y), HitZone::Content))
    }

    /// Drop any selection and its highlight; true if there was one.
    pub fn clear_selection(&mut self) -> bool {
        let had = self.selection.is_some();
        self.selection = None;
        for pane in &mut self.panes {
            if pane.highlight.is_some() {
                pane.highlight = None;
                pane.screen.mark_all_dirty();
            }
        }
        had
    }

    /// Anchor a new selection in the pane under the cursor.
    pub fn begin_selection(&mut self, x: usize, y: usize) {
        let Some(idx) = self.content_pane_at(x, y) else {
            return;
        };
        let Some(pane) = self.panes.get(idx) else {
            return;
        };
        let rect = pane.frame.content_rect();
        let (col, row) = selection::clamp_to(&rect, x, y);
        self.selection = Some(Selection::start(idx, pane.scroll_offset, col, row));
    }

    /// Move the selection's free end; true if the highlight changed.
    pub fn extend_selection(&mut self, x: usize, y: usize) -> bool {
        let Some(mut sel) = self.selection else {
            return false;
        };
        let Some(pane) = self.panes.get(sel.pane_idx) else {
            return false;
        };
        // why: clamp to the anchor pane so a drag never spills past a border.
        let rect = pane.frame.content_rect();
        let (col, row) = selection::clamp_to(&rect, x, y);
        sel.extend_to(col, row);
        if self.selection == Some(sel) {
            return false;
        }
        self.selection = Some(sel);
        let visible = if sel.is_empty() { None } else { Some(sel) };
        if let Some(pane) = self.panes.get_mut(sel.pane_idx) {
            pane.highlight = visible;
            pane.screen.mark_all_dirty();
        }
        true
    }

    /// Repaint the pane that holds the selection.
    pub fn render_selected_pane(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        let Some(sel) = self.selection else {
            return Ok(());
        };
        if let Some(pane) = self.panes.get_mut(sel.pane_idx)
            && !pane.is_service()
        {
            pane.render_full(out)?;
        }
        out.flush()
    }

    /// Copy the visible text of the pane whose copy button was clicked.
    pub fn copy_visible_pane(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        let Some(idx) = self.copy_requested.take() else {
            return Ok(());
        };
        let Some(pane) = self.panes.get(idx) else {
            return Ok(());
        };
        let text = pane.visible_text();
        if text.trim().is_empty() {
            return Ok(());
        }
        selection::copy_to_host_clipboard(&text);
        out.write_all(&selection::osc52(&text))?;
        out.flush()
    }

    /// Copy the selected text to the host clipboard.
    pub fn copy_selection(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        let Some(sel) = self.selection else {
            return Ok(());
        };
        if sel.is_empty() {
            return Ok(());
        }
        let Some(pane) = self.panes.get(sel.pane_idx) else {
            return Ok(());
        };
        let text = sel.text(&pane.screen);
        if text.trim().is_empty() {
            return Ok(());
        }
        selection::copy_to_host_clipboard(&text);
        out.write_all(&selection::osc52(&text))?;
        out.flush()
    }
}
