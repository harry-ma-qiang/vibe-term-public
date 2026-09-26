// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Terminal emulator: cell grid, styles, scrollback and the vte handler.

use unicode_width::UnicodeWidthChar;
use vte::ansi::{
    Attr, CharsetIndex, ClearMode, Color, Handler, LineClearMode, NamedPrivateMode, PrivateMode,
    Processor, StandardCharset,
};

/// A cell colour: terminal default, palette index, or direct rgb.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Rendition attributes applied to a cell.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct CellStyle {
    pub fg: TermColor,
    pub bg: TermColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub hidden: bool,
}

/// One grid cell: a character, its display width and its style.
#[derive(Clone, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub width: u8,
    pub style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            width: 1,
            style: CellStyle::default(),
        }
    }
}

const MAX_SCROLLBACK: usize = 2000;

/// The emulated screen: visible grid, cursor, dirty rows and scrollback.
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<Cell>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub dirty: Vec<bool>,
    pub wrapped: Vec<bool>,
    current_style: CellStyle,
    processor: Processor,
    scroll_top: usize,
    scroll_bottom: usize,
    saved_cursor: Option<(usize, usize, CellStyle)>,
    alt_cells: Option<Vec<Vec<Cell>>>,
    alt_cursor: Option<(usize, usize)>,
    scrollback: Vec<Vec<Cell>>,
    scrollback_wrapped: Vec<bool>,
    charsets: [StandardCharset; 4],
    active_charset: CharsetIndex,
    autowrap: bool,
}

impl Screen {
    /// Create a blank screen of `cols` by `rows` cells.
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![vec![Cell::default(); cols]; rows],
            cursor_row: 0,
            cursor_col: 0,
            dirty: vec![true; rows],
            wrapped: vec![false; rows],
            current_style: CellStyle::default(),
            processor: Processor::new(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            saved_cursor: None,
            alt_cells: None,
            alt_cursor: None,
            scrollback: Vec::new(),
            scrollback_wrapped: Vec::new(),
            charsets: [StandardCharset::Ascii; 4],
            active_charset: CharsetIndex::G0,
            autowrap: true,
        }
    }

    /// Feed raw pty bytes through the escape-sequence parser.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut processor = std::mem::replace(&mut self.processor, Processor::new());
        processor.advance(self, bytes);
        self.processor = processor;
    }

    /// The visible text of `row`, trailing blanks trimmed.
    pub fn row_text(&self, row: usize) -> String {
        self.cells[row]
            .iter()
            .filter(|c| c.width > 0)
            .map(|c| c.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// Mark every row clean after a render pass.
    pub fn clear_dirty(&mut self) {
        self.dirty.iter_mut().for_each(|d| *d = false);
    }

    /// Mark every row dirty to force a full redraw.
    pub fn mark_all_dirty(&mut self) {
        self.dirty.iter_mut().for_each(|d| *d = true);
    }

    /// Number of rows currently held in scrollback.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// The scrollback row at `idx`, oldest first.
    pub fn scrollback_row(&self, idx: usize) -> Option<&Vec<Cell>> {
        self.scrollback.get(idx)
    }

    /// Whether the scrollback row at `idx` continues onto the next row.
    pub fn scrollback_is_wrapped(&self, idx: usize) -> bool {
        self.scrollback_wrapped.get(idx).copied().unwrap_or(false)
    }

    /// The text of the scrollback row at `idx`, trailing blanks trimmed.
    pub fn scrollback_row_text(&self, idx: usize) -> Option<String> {
        self.scrollback.get(idx).map(|row| {
            row.iter()
                .filter(|c| c.width > 0)
                .map(|c| c.ch)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
    }

    /// Scrollback rows plus visible rows.
    pub fn total_lines(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    /// True while the alternate screen is active.
    pub fn is_alt_screen(&self) -> bool {
        self.alt_cells.is_some()
    }

    /// Resize the grid, growing rows and columns as needed.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        if self.cells.len() < rows {
            self.cells.resize(rows, vec![Cell::default(); cols]);
        }
        for row in &mut self.cells {
            if row.len() < cols {
                row.resize(cols, Cell::default());
            }
        }
        // why: pad scrollback on grow; the renderer stops at row length.
        for row in &mut self.scrollback {
            if row.len() < cols {
                row.resize(cols, Cell::default());
            }
        }
        self.dirty = vec![true; rows];
        self.wrapped.resize(rows, false);
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        if self.cursor_row >= rows {
            self.cursor_row = rows - 1;
        }
        if self.cursor_col >= cols {
            self.cursor_col = cols - 1;
        }
    }

    fn scroll_region_up(&mut self) {
        let was_wrapped = self.wrapped[self.scroll_top];
        let removed = self.cells.remove(self.scroll_top);
        if self.scroll_top == 0 && self.alt_cells.is_none() {
            self.scrollback.push(removed);
            self.scrollback_wrapped.push(was_wrapped);
            if self.scrollback.len() > MAX_SCROLLBACK {
                self.scrollback.remove(0);
                self.scrollback_wrapped.remove(0);
            }
        }
        self.wrapped.remove(self.scroll_top);
        self.wrapped.insert(self.scroll_bottom, false);
        self.cells
            .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
        for r in self.scroll_top..=self.scroll_bottom {
            self.dirty[r] = true;
        }
    }

    fn enter_alt_screen(&mut self) {
        if self.alt_cells.is_some() {
            return;
        }
        self.alt_cells = Some(self.cells.clone());
        self.alt_cursor = Some((self.cursor_row, self.cursor_col));
        for row in 0..self.rows {
            for col in 0..self.cols {
                self.cells[row][col] = Cell::default();
            }
            self.dirty[row] = true;
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    fn leave_alt_screen(&mut self) {
        if let Some(mut saved) = self.alt_cells.take() {
            saved.resize(self.rows, vec![Cell::default(); self.cols]);
            for row in &mut saved {
                row.resize(self.cols, Cell::default());
            }
            self.cells = saved;
            if let Some((r, c)) = self.alt_cursor.take() {
                self.cursor_row = r.min(self.rows.saturating_sub(1));
                self.cursor_col = c.min(self.cols.saturating_sub(1));
            }
            self.dirty.iter_mut().for_each(|d| *d = true);
        }
    }

    fn scroll_region_down(&mut self) {
        self.cells.remove(self.scroll_bottom);
        self.wrapped.remove(self.scroll_bottom);
        self.cells
            .insert(self.scroll_top, vec![Cell::default(); self.cols]);
        self.wrapped.insert(self.scroll_top, false);
        for r in self.scroll_top..=self.scroll_bottom {
            self.dirty[r] = true;
        }
    }
}

impl Handler for Screen {
    fn input(&mut self, c: char) {
        let c = self.charsets[self.active_charset as usize].map(c);
        let w = c.width().unwrap_or(1);
        if self.cursor_col + w > self.cols {
            // why: with DECAWM off the cursor pins to the right margin instead.
            if !self.autowrap {
                if w > self.cols {
                    return;
                }
                self.cursor_col = self.cols - w;
            } else {
                self.wrapped[self.cursor_row] = true;
                self.cursor_col = 0;
                if self.cursor_row == self.scroll_bottom {
                    self.scroll_region_up();
                } else if self.cursor_row < self.rows - 1 {
                    self.cursor_row += 1;
                }
            }
        }
        let row = self.cursor_row;
        let col = self.cursor_col;
        self.cells[row][col] = Cell {
            ch: c,
            width: w as u8,
            style: self.current_style,
        };
        for i in 1..w {
            if col + i < self.cols {
                self.cells[row][col + i] = Cell {
                    ch: ' ',
                    width: 0,
                    style: self.current_style,
                };
            }
        }
        self.dirty[row] = true;
        self.cursor_col += w;
    }

    fn goto(&mut self, line: i32, col: usize) {
        self.cursor_row = (line.max(0) as usize).min(self.rows.saturating_sub(1));
        self.cursor_col = col.min(self.cols.saturating_sub(1));
    }

    fn goto_line(&mut self, line: i32) {
        self.cursor_row = (line.max(0) as usize).min(self.rows.saturating_sub(1));
    }

    fn goto_col(&mut self, col: usize) {
        self.cursor_col = col.min(self.cols.saturating_sub(1));
    }

    fn move_up(&mut self, n: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(n);
    }

    fn move_down(&mut self, n: usize) {
        self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
    }

    fn move_forward(&mut self, n: usize) {
        self.cursor_col = (self.cursor_col + n).min(self.cols.saturating_sub(1));
    }

    fn move_backward(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(n);
    }

    fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    fn linefeed(&mut self) {
        self.wrapped[self.cursor_row] = false;
        if self.cursor_row == self.scroll_bottom {
            self.scroll_region_up();
        } else if self.cursor_row < self.rows - 1 {
            self.cursor_row += 1;
        }
    }

    fn newline(&mut self) {
        self.linefeed();
        self.carriage_return();
    }

    fn put_tab(&mut self, count: u16) {
        for _ in 0..count {
            let next = (self.cursor_col + 8) / 8 * 8;
            self.cursor_col = next.min(self.cols.saturating_sub(1));
        }
    }

    fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    fn erase_chars(&mut self, n: usize) {
        let row = self.cursor_row;
        for i in 0..n {
            let col = self.cursor_col + i;
            if col < self.cols {
                self.cells[row][col] = Cell::default();
            }
        }
        self.dirty[row] = true;
    }

    fn delete_chars(&mut self, n: usize) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        for i in col..self.cols {
            let src = i + n;
            self.cells[row][i] = if src < self.cols {
                self.cells[row][src].clone()
            } else {
                Cell::default()
            };
        }
        self.dirty[row] = true;
    }

    fn clear_line(&mut self, mode: LineClearMode) {
        let row = self.cursor_row;
        let (start, end) = match mode {
            LineClearMode::Right => (self.cursor_col, self.cols),
            LineClearMode::Left => (0, self.cursor_col + 1),
            LineClearMode::All => (0, self.cols),
        };
        for col in start..end {
            self.cells[row][col] = Cell::default();
        }
        self.dirty[row] = true;
    }

    fn clear_screen(&mut self, mode: ClearMode) {
        match mode {
            ClearMode::All | ClearMode::Saved => {
                for row in 0..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                    self.dirty[row] = true;
                }
            }
            ClearMode::Below => {
                self.clear_line(LineClearMode::Right);
                for row in (self.cursor_row + 1)..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                    self.dirty[row] = true;
                }
            }
            ClearMode::Above => {
                self.clear_line(LineClearMode::Left);
                for row in 0..self.cursor_row {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                    self.dirty[row] = true;
                }
            }
        }
    }

    fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            self.scroll_region_up();
        }
    }

    fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            self.scroll_region_down();
        }
    }

    fn insert_blank_lines(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor_row < self.rows {
                self.cells
                    .insert(self.cursor_row, vec![Cell::default(); self.cols]);
                self.cells.truncate(self.rows);
            }
        }
        self.dirty.iter_mut().for_each(|d| *d = true);
    }

    fn insert_blank(&mut self, n: usize) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        for _ in 0..n {
            if col < self.cols {
                self.cells[row].insert(col, Cell::default());
                self.cells[row].truncate(self.cols);
            }
        }
        self.dirty[row] = true;
    }

    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        let t = top.saturating_sub(1);
        let b = bottom
            .map(|b| b.saturating_sub(1))
            .unwrap_or(self.rows.saturating_sub(1));
        if t < self.rows && b < self.rows && t < b {
            self.scroll_top = t;
            self.scroll_bottom = b;
        } else if t == 0 && b == 0 {
            self.scroll_top = 0;
            self.scroll_bottom = self.rows.saturating_sub(1);
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    fn save_cursor_position(&mut self) {
        self.saved_cursor = Some((self.cursor_row, self.cursor_col, self.current_style));
    }

    fn restore_cursor_position(&mut self) {
        if let Some((row, col, style)) = self.saved_cursor {
            self.cursor_row = row.min(self.rows.saturating_sub(1));
            self.cursor_col = col.min(self.cols.saturating_sub(1));
            self.current_style = style;
        }
    }

    fn reverse_index(&mut self) {
        if self.cursor_row == self.scroll_top {
            self.scroll_region_down();
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
    }

    fn delete_lines(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor_row <= self.scroll_bottom {
                self.cells.remove(self.cursor_row);
                self.cells
                    .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
            }
        }
        for r in self.cursor_row..=self.scroll_bottom {
            self.dirty[r] = true;
        }
    }

    fn terminal_attribute(&mut self, attr: Attr) {
        match attr {
            Attr::Reset => self.current_style = CellStyle::default(),
            Attr::Bold => self.current_style.bold = true,
            Attr::Dim => self.current_style.dim = true,
            Attr::Italic => self.current_style.italic = true,
            Attr::Underline => self.current_style.underline = true,
            Attr::Reverse => self.current_style.reverse = true,
            Attr::Hidden => self.current_style.hidden = true,
            Attr::CancelBold | Attr::CancelBoldDim => {
                self.current_style.bold = false;
                self.current_style.dim = false;
            }
            Attr::CancelItalic => self.current_style.italic = false,
            Attr::CancelUnderline => self.current_style.underline = false,
            Attr::CancelReverse => self.current_style.reverse = false,
            Attr::CancelHidden => self.current_style.hidden = false,
            Attr::Foreground(color) => self.current_style.fg = convert_color(color),
            Attr::Background(color) => self.current_style.bg = convert_color(color),
            _ => {}
        }
    }

    fn set_private_mode(&mut self, mode: PrivateMode) {
        if matches!(
            mode,
            PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor)
        ) {
            self.enter_alt_screen();
        }
        if matches!(mode, PrivateMode::Named(NamedPrivateMode::LineWrap)) {
            self.autowrap = true;
        }
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        if matches!(
            mode,
            PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor)
        ) {
            self.leave_alt_screen();
        }
        if matches!(mode, PrivateMode::Named(NamedPrivateMode::LineWrap)) {
            self.autowrap = false;
        }
    }

    fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
        self.charsets[index as usize] = charset;
    }

    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.active_charset = index;
    }
}

fn convert_color(color: Color) -> TermColor {
    match color {
        Color::Named(n) => {
            let v = n as u16;
            if v < 16 {
                TermColor::Indexed(v as u8)
            } else if v >= 256 {
                TermColor::Default
            } else {
                TermColor::Indexed(v as u8)
            }
        }
        Color::Indexed(i) => TermColor::Indexed(i),
        Color::Spec(rgb) => TermColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

#[cfg(test)]
#[path = "tests/emulator.rs"]
mod tests;
