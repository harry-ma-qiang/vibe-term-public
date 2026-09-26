// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Screen painter: turns emulator cells into escape sequences, either a full
//! repaint or a diff against the previously drawn frame.

use crate::emulator::{Cell, Screen, TermColor};
use std::io::{self, Write};

/// Holds the last painted frame so the next one can be drawn as a diff.
pub struct Renderer {
    prev: Vec<Vec<Cell>>,
    prev_cursor: (usize, usize),
    cols: usize,
    rows: usize,
    row_offset: usize,
    col_offset: usize,
}

impl Renderer {
    /// Build a renderer for a viewport of the given size.
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            prev: vec![vec![Cell::default(); cols]; rows],
            prev_cursor: (0, 0),
            cols,
            rows,
            row_offset: 0,
            col_offset: 0,
        }
    }

    /// Place the viewport's top left corner on the host terminal.
    pub fn set_offset(&mut self, row_offset: usize, col_offset: usize) {
        self.row_offset = row_offset;
        self.col_offset = col_offset;
    }

    /// Repaint every row of the screen and leave the cursor where it belongs.
    pub fn render_full(&mut self, screen: &mut Screen, out: &mut impl Write) -> io::Result<()> {
        out.write_all(b"\x1b[?25l")?;
        for row in 0..screen.rows.min(self.rows).min(screen.cells.len()) {
            let term_row = row + self.row_offset + 1;
            let term_col = self.col_offset + 1;
            out.write_all(format!("\x1b[{term_row};{term_col}H").as_bytes())?;
            write_row(out, &screen.cells[row], screen.cols)?;
        }
        write_cursor(
            out,
            screen.cursor_row + self.row_offset,
            screen.cursor_col + self.col_offset,
        )?;
        out.write_all(b"\x1b[?25h")?;
        out.flush()?;
        self.snapshot(screen);
        Ok(())
    }

    /// Repaint only the rows that are dirty and actually differ.
    pub fn render_diff(&mut self, screen: &mut Screen, out: &mut impl Write) -> io::Result<()> {
        let mut changed = false;
        out.write_all(b"\x1b[?25l")?;

        let max_row = screen
            .rows
            .min(self.rows)
            .min(screen.cells.len())
            .min(screen.dirty.len());
        for row in 0..max_row {
            if !screen.dirty[row] {
                continue;
            }
            if row < self.prev.len() && self.prev[row] == screen.cells[row] {
                continue;
            }
            let term_row = row + self.row_offset + 1;
            let term_col = self.col_offset + 1;
            out.write_all(format!("\x1b[{term_row};{term_col}H").as_bytes())?;
            write_row(out, &screen.cells[row], screen.cols)?;
            changed = true;
        }

        let new_cursor = (screen.cursor_row, screen.cursor_col);
        if changed || new_cursor != self.prev_cursor {
            write_cursor(
                out,
                screen.cursor_row + self.row_offset,
                screen.cursor_col + self.col_offset,
            )?;
        }

        out.write_all(b"\x1b[?25h")?;
        out.flush()?;
        self.snapshot(screen);
        Ok(())
    }

    /// Resize the viewport and drop the previous frame.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.prev = vec![vec![Cell::default(); cols]; rows];
    }

    /// Forget the previous frame so the next diff repaints everything.
    pub fn invalidate(&mut self) {
        for row in &mut self.prev {
            for cell in row {
                *cell = Cell::default();
            }
        }
    }

    fn snapshot(&mut self, screen: &mut Screen) {
        self.prev = screen.cells.clone();
        self.prev_cursor = (screen.cursor_row, screen.cursor_col);
        screen.clear_dirty();
    }
}

/// Write one row of cells, emitting a style sequence only when it changes.
pub fn write_row(out: &mut impl Write, cells: &[Cell], cols: usize) -> io::Result<()> {
    let mut current_seq = String::new();

    for cell in cells.iter().take(cols) {
        if cell.width == 0 {
            continue;
        }

        let seq = style_sequence(&cell.style);
        if seq != current_seq {
            if !current_seq.is_empty() {
                out.write_all(b"\x1b[0m")?;
            }
            if !seq.is_empty() {
                out.write_all(seq.as_bytes())?;
            }
            current_seq = seq;
        }

        let mut buf = [0u8; 4];
        let s = cell.ch.encode_utf8(&mut buf);
        out.write_all(s.as_bytes())?;
    }

    if !current_seq.is_empty() {
        out.write_all(b"\x1b[0m")?;
    }
    Ok(())
}

fn write_cursor(out: &mut impl Write, row: usize, col: usize) -> io::Result<()> {
    write!(out, "\x1b[{};{}H", row + 1, col + 1)
}

fn style_sequence(style: &crate::emulator::CellStyle) -> String {
    let mut codes = Vec::new();
    if style.bold {
        codes.push("1".to_string());
    }
    if style.dim {
        codes.push("2".to_string());
    }
    if style.italic {
        codes.push("3".to_string());
    }
    if style.underline {
        codes.push("4".to_string());
    }
    if style.reverse {
        codes.push("7".to_string());
    }
    if style.hidden {
        codes.push("8".to_string());
    }
    match style.fg {
        TermColor::Indexed(i) if i < 8 => codes.push(format!("{}", 30 + i)),
        TermColor::Indexed(i) if i < 16 => codes.push(format!("{}", 90 + i - 8)),
        TermColor::Indexed(i) => codes.push(format!("38;5;{i}")),
        TermColor::Rgb(r, g, b) => codes.push(format!("38;2;{r};{g};{b}")),
        TermColor::Default => {}
    }
    match style.bg {
        TermColor::Indexed(i) if i < 8 => codes.push(format!("{}", 40 + i)),
        TermColor::Indexed(i) if i < 16 => codes.push(format!("{}", 100 + i - 8)),
        TermColor::Indexed(i) => codes.push(format!("48;5;{i}")),
        TermColor::Rgb(r, g, b) => codes.push(format!("48;2;{r};{g};{b}")),
        TermColor::Default => {}
    }
    if codes.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

#[cfg(test)]
#[path = "tests/renderer.rs"]
mod tests;
