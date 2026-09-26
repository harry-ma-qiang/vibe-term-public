// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Painting a whole session: borders, the status bar, every pane, the cursor,
//! and the composited screen the recorder writes.

use crate::emulator::Screen;
use crate::pane::Pane;
use crate::session::Session;
use crate::status_bar;
use std::io::Write;

impl Session {
    /// Draw the border of every pane, marking the focused one.
    pub fn render_all_borders(&self, out: &mut impl Write) -> std::io::Result<()> {
        for (i, pane) in self.panes.iter().enumerate() {
            let close_hover = self.hovered_close == Some(i);
            let copy_hover = self.hovered_copy == Some(i);
            pane.render_border(out, i == self.focused, close_hover, copy_hover)?;
        }
        out.flush()
    }

    /// Draw the two-row status bar over its own rectangle.
    pub fn render_status_bar(&self, out: &mut impl Write) -> std::io::Result<()> {
        let layout = self.compute_layout();
        let r = layout.status_rect;
        let (bridge_on, bridge_status, bridge_error) =
            (false, status_bar::BridgeStatusWord::Disabled, None);
        status_bar::render(
            out,
            &status_bar::RenderArgs {
                area: r,
                hover_x: self.hover_x,
                hover_y: self.hover_y,
                hover_on_bar: self.hover_y >= r.y && self.hover_y < r.y + r.h,
                sidebar_count: self.sidebar_count,
                split_ratio: self.split_ratio,
                filter_enabled: self.filter_enabled,
                stage_mode: self.stage_mode,
                headless: self.headless,
                active_children: self.sidebar_running_count(),
                bridge_on,
                bridge_status,
                quit_pending: self.quit_toast_active(),
                bridge_error,
            },
        )
    }

    /// Repaint every pane in full, then place the cursor.
    pub fn force_render_all(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        for pane in &mut self.panes {
            if pane.is_service() {
                let mut buf = Vec::new();
                pane.render_service(&mut buf);
                if !buf.is_empty() {
                    out.write_all(&buf)?;
                }
                continue;
            }
            let _ = pane.render_full(out);
        }
        self.position_cursor(out)
    }

    /// Move the host cursor to the focused pane's cursor cell.
    pub fn position_cursor(&self, out: &mut impl Write) -> std::io::Result<()> {
        if let Some(pane) = self.panes.get(self.focused) {
            let content = pane.frame.content_rect();
            let row = pane.screen.cursor_row + content.y + 1;
            let col = pane.screen.cursor_col + content.x + 1;
            write!(out, "\x1b[{row};{col}H")?;
        }
        out.flush()
    }

    /// Clear the screen and repaint borders, status bar and every pane.
    pub fn composite_frame(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        out.write_all(b"\x1b[2J")?;
        self.render_all_borders(out)?;
        self.render_status_bar(out)?;
        self.force_render_all(out)?;
        Ok(())
    }

    /// Read from every pane, paint what changed, and record a frame.
    pub fn poll_and_render(&mut self, out: &mut impl Write) -> std::io::Result<bool> {
        let mut any_output = false;
        for pane in &mut self.panes {
            if pane.is_service() {
                pane.tick_service();
                let mut buf = Vec::new();
                pane.render_service(&mut buf);
                if !buf.is_empty() && out.write_all(&buf).is_ok() {
                    any_output = true;
                }
                continue;
            }
            match pane.feed_output() {
                Ok(true) => {
                    pane.flush_log();
                    if pane.render(out).is_err() {
                        continue;
                    }
                    any_output = true;
                }
                Ok(false) => {}
                Err(_) => continue,
            }
        }
        if any_output {
            if self.recorder.is_some() {
                let status = self.status_bar_text();
                let mut comp = composite_panes(
                    &self.panes,
                    self.term_cols,
                    self.term_rows,
                    self.focused,
                    &status,
                );
                if self.filter_enabled
                    && let Some(main) = self.panes.first()
                {
                    main.filter.scrub_screen(&mut comp);
                }
                if let Some(ref mut rec) = self.recorder {
                    rec.record_frame(&comp)?;
                }
            }
            self.position_cursor(out)?;
        }
        Ok(any_output)
    }
}

/// Stamp every pty pane and the status text into one offscreen screen.
pub fn composite_panes(
    panes: &[Pane],
    cols: usize,
    rows: usize,
    focused: usize,
    status_text: &str,
) -> Screen {
    let mut comp = Screen::new(cols, rows);
    for (i, pane) in panes.iter().enumerate() {
        if pane.is_service() {
            continue;
        }
        crate::frame::stamp_border(&pane.frame, &mut comp, i == focused);
        let content = pane.frame.content_rect();
        let max_row = pane.screen.rows.min(content.h).min(pane.screen.cells.len());
        for row in 0..max_row {
            let max_col = pane
                .screen
                .cols
                .min(content.w)
                .min(pane.screen.cells[row].len());
            for col in 0..max_col {
                let tr = content.y + row;
                let tc = content.x + col;
                if tr < comp.rows && tc < comp.cols {
                    comp.cells[tr][tc] = pane.screen.cells[row][col].clone();
                }
            }
        }
    }
    let status_row = rows.saturating_sub(1);
    if status_row < comp.rows {
        use crate::emulator::{Cell, CellStyle, TermColor};
        let dim = CellStyle {
            fg: TermColor::Indexed(8),
            ..CellStyle::default()
        };
        for (i, ch) in status_text.chars().enumerate() {
            if i < cols {
                comp.cells[status_row][i] = Cell {
                    ch,
                    width: 1,
                    style: dim,
                };
            }
        }
    }
    for r in 0..comp.rows {
        comp.dirty[r] = true;
    }
    comp
}
