// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Painting one pane: diff and full repaints, the scrollback view, and the
//! non-destructive redaction and selection overlays.

use crate::pane::Pane;
use std::io::{self, Write};

impl Pane {
    /// Paint only the cells that changed since the last paint.
    pub fn render(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.filter.is_enabled() || self.highlight.is_some() {
            let backup = self.screen.cells.clone();
            if self.filter.is_enabled() {
                self.filter.scrub_screen(&mut self.screen);
            }
            self.paint_highlight();
            let result = if self.scroll_offset > 0 {
                self.render_scrolled(out)
            } else {
                self.renderer.render_diff(&mut self.screen, out)
            };
            self.screen.cells = backup;
            return result;
        }
        if self.scroll_offset > 0 {
            self.render_scrolled(out)
        } else {
            self.renderer.render_diff(&mut self.screen, out)
        }
    }

    /// Paint every cell of the pane, ignoring the dirty map.
    pub fn render_full(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.filter.is_enabled() || self.highlight.is_some() {
            let backup = self.screen.cells.clone();
            if self.filter.is_enabled() {
                self.filter.scrub_screen(&mut self.screen);
            }
            self.paint_highlight();
            let result = if self.scroll_offset > 0 {
                self.render_scrolled(out)
            } else {
                self.renderer.render_full(&mut self.screen, out)
            };
            self.screen.cells = backup;
            return result;
        }
        self.renderer.render_full(&mut self.screen, out)
    }

    fn paint_highlight(&mut self) {
        let Some(sel) = self.highlight else {
            return;
        };
        if self.scroll_offset > 0 {
            return;
        }
        let rows = self.screen.rows.min(self.screen.cells.len());
        for row in 0..rows {
            for col in 0..self.screen.cols {
                if sel.contains(col, row)
                    && let Some(cell) = self.screen.cells[row].get_mut(col)
                {
                    cell.style.reverse = !cell.style.reverse;
                }
            }
        }
    }

    fn render_scrolled(&self, out: &mut impl Write) -> io::Result<()> {
        let content = self.frame.content_rect();
        let sb_len = self.screen.scrollback_len();
        let view_start = sb_len.saturating_sub(self.scroll_offset);
        out.write_all(b"\x1b[?25l")?;
        for row in 0..self.screen.rows {
            let abs_line = view_start + row;
            let term_row = row + content.y + 1;
            let term_col = content.x + 1;
            write!(out, "\x1b[{term_row};{term_col}H")?;
            let cells = if abs_line < sb_len {
                self.screen.scrollback_row(abs_line)
            } else {
                let screen_row = abs_line - sb_len;
                if screen_row < self.screen.rows {
                    Some(&self.screen.cells[screen_row])
                } else {
                    None
                }
            };
            if let Some(cells) = cells {
                match self.highlight {
                    Some(sel) => {
                        let mut owned = cells.clone();
                        for (col, cell) in owned.iter_mut().enumerate() {
                            if sel.contains(col, row) {
                                cell.style.reverse = !cell.style.reverse;
                            }
                        }
                        crate::renderer::write_row(out, &owned, self.screen.cols)?;
                    }
                    None => crate::renderer::write_row(out, cells, self.screen.cols)?,
                }
            }
        }
        out.write_all(b"\x1b[?25h")?;
        out.flush()
    }
}
