// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Pane geometry and chrome: rectangles, hit testing and border drawing,
//! either as an escape-sequence string or stamped into a screen grid.

/// Label drawn for the copy button in a pane title bar.
pub const COPY_LABEL: &str = "[c]";
const COPY_LABEL_WIDTH: usize = 3;
const CLOSE_LABEL_WIDTH: usize = 3;
const BUTTONS_WIDTH: usize = COPY_LABEL_WIDTH + CLOSE_LABEL_WIDTH;

/// A rectangle in terminal cells, positioned from the top left.
#[derive(Clone, Copy)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// One pane's outer rectangle plus the slot and pid shown in its title.
#[derive(Clone)]
pub struct Frame {
    pub outer: Rect,
    pub slot: usize,
    pub pid: i32,
}

impl Frame {
    /// Build a frame from its outer rectangle, slot number and child pid.
    pub fn new(outer: Rect, slot: usize, pid: i32) -> Self {
        Self { outer, slot, pid }
    }

    /// The rectangle inside the border, where child output is drawn.
    pub fn content_rect(&self) -> Rect {
        Rect {
            x: self.outer.x + 1,
            y: self.outer.y + 1,
            w: self.outer.w.saturating_sub(2),
            h: self.outer.h.saturating_sub(2),
        }
    }

    /// Columns and rows to give the child pseudoterminal.
    pub fn child_size(&self) -> (usize, usize) {
        let c = self.content_rect();
        (c.w, c.h)
    }

    /// Classify a terminal cell as content, title bar, a button or border.
    pub fn hit_test(&self, x: usize, y: usize) -> HitZone {
        if x < self.outer.x
            || x >= self.outer.x + self.outer.w
            || y < self.outer.y
            || y >= self.outer.y + self.outer.h
        {
            return HitZone::Outside;
        }
        if y == self.outer.y {
            // why: mirrors render_border_with_title, whose button run ends one cell before the corner.
            let buttons_start = self.outer.x + self.outer.w - 1 - BUTTONS_WIDTH;
            let close_start = buttons_start + COPY_LABEL_WIDTH;
            if x >= close_start && x < close_start + CLOSE_LABEL_WIDTH {
                return HitZone::CloseButton;
            }
            if x >= buttons_start && x < close_start {
                return HitZone::CopyButton;
            }
            return HitZone::TitleBar;
        }
        if y == self.outer.y + self.outer.h - 1
            || x == self.outer.x
            || x == self.outer.x + self.outer.w - 1
        {
            return HitZone::Border;
        }
        HitZone::Content
    }

    /// Append the default border, titled with the slot and pid, to a buffer.
    pub fn render_border(&self, buf: &mut String, is_focused: bool, close_hovered: bool) {
        self.render_border_with_title(
            buf,
            BorderOpts {
                is_focused,
                close_hovered,
                copy_hovered: false,
                title_override: None,
                close_label: "[x]",
            },
        );
    }

    /// Append the border to a buffer, with title and button state from `opts`.
    pub fn render_border_with_title(&self, buf: &mut String, opts: BorderOpts<'_>) {
        let BorderOpts {
            is_focused,
            close_hovered,
            copy_hovered,
            title_override,
            close_label,
        } = opts;
        let x = self.outer.x;
        let y = self.outer.y;
        let w = self.outer.w;
        let h = self.outer.h;

        if w < 4 || h < 3 {
            return;
        }

        let title = match title_override {
            Some(t) => format!(" {} ", t),
            None => format!(" #{} pid:{} ", self.slot, self.pid),
        };
        let close = close_label;
        let buttons_width = BUTTONS_WIDTH;
        let title_space = w.saturating_sub(2);
        let border_style = if is_focused { "\x1b[1;36m" } else { "\x1b[90m" };
        let reset = "\x1b[0m";

        goto(buf, x, y);
        buf.push_str(border_style);
        buf.push('┌');
        let mut filled = 0;
        let max_title = title_space.saturating_sub(buttons_width + 1);
        if title.len() <= max_title {
            buf.push('─');
            buf.push_str(&title);
            filled = 1 + title.len();
        } else if max_title > 4 {
            buf.push('─');
            buf.push_str(&title[..max_title - 2]);
            buf.push_str("..");
            filled = 1 + max_title;
        }
        let close_start = title_space.saturating_sub(buttons_width);
        while filled < close_start {
            buf.push('─');
            filled += 1;
        }
        if copy_hovered {
            buf.push_str("\x1b[1;32;7m");
            buf.push_str(COPY_LABEL);
            buf.push_str("\x1b[0m");
            buf.push_str(border_style);
        } else {
            buf.push_str(COPY_LABEL);
        }
        if close_hovered {
            buf.push_str("\x1b[1;31;7m");
            buf.push_str(close);
            buf.push_str("\x1b[0m");
            buf.push_str(border_style);
        } else {
            buf.push_str(close);
        }
        filled += buttons_width;
        while filled < title_space {
            buf.push('─');
            filled += 1;
        }
        buf.push('┐');
        buf.push_str(reset);

        for row in 1..h.saturating_sub(1) {
            goto(buf, x, y + row);
            buf.push_str(border_style);
            buf.push('│');
            buf.push_str(reset);
            goto(buf, x + w - 1, y + row);
            buf.push_str(border_style);
            buf.push('│');
            buf.push_str(reset);
        }

        goto(buf, x, y + h - 1);
        buf.push_str(border_style);
        buf.push('└');
        for _ in 0..title_space {
            buf.push('─');
        }
        buf.push('┘');
        buf.push_str(reset);
    }
}

/// Which part of a pane a terminal cell belongs to.
#[derive(Debug, PartialEq)]
pub enum HitZone {
    Content,
    TitleBar,
    CloseButton,
    CopyButton,
    Border,
    Outside,
}

/// Options bundle for `Frame::render_border_with_title`, keeping its argument
/// count under clippy's cap and letting callers name each value.
pub struct BorderOpts<'a> {
    pub is_focused: bool,
    pub close_hovered: bool,
    pub copy_hovered: bool,
    pub title_override: Option<&'a str>,
    pub close_label: &'a str,
}

/// Write a pane border into a screen grid instead of an escape string.
pub fn stamp_border(frame: &Frame, screen: &mut crate::emulator::Screen, focused: bool) {
    use crate::emulator::{CellStyle, TermColor};
    let x = frame.outer.x;
    let y = frame.outer.y;
    let w = frame.outer.w;
    let h = frame.outer.h;
    if w < 4 || h < 3 {
        return;
    }

    let fg = if focused {
        TermColor::Indexed(6)
    } else {
        TermColor::Indexed(8)
    };
    let style = CellStyle {
        fg,
        bold: focused,
        ..CellStyle::default()
    };

    let set = |s: &mut crate::emulator::Screen, r: usize, c: usize, ch: char| {
        if r < s.rows && c < s.cols {
            s.cells[r][c].ch = ch;
            s.cells[r][c].width = 1;
            s.cells[r][c].style = style;
        }
    };

    set(screen, y, x, '┌');
    set(screen, y, x + w - 1, '┐');
    set(screen, y + h - 1, x, '└');
    set(screen, y + h - 1, x + w - 1, '┘');

    for c in 1..w - 1 {
        set(screen, y, x + c, '─');
        set(screen, y + h - 1, x + c, '─');
    }
    for r in 1..h - 1 {
        set(screen, y + r, x, '│');
        set(screen, y + r, x + w - 1, '│');
    }

    let title = format!(" #{} pid:{} ", frame.slot, frame.pid);
    let close = "[x]";
    let title_space = w.saturating_sub(2);
    let mut col = x + 1;
    if title.len() < title_space.saturating_sub(close.len()) {
        set(screen, y, col, '─');
        col += 1;
        for ch in title.chars() {
            set(screen, y, col, ch);
            col += 1;
        }
    }
    let close_col = x + w - 1 - close.len();
    for (i, ch) in close.chars().enumerate() {
        set(screen, y, close_col + i, ch);
    }
}

fn goto(buf: &mut String, x: usize, y: usize) {
    use std::fmt::Write;
    let _ = write!(buf, "\x1b[{};{}H", y + 1, x + 1);
}

#[cfg(test)]
#[path = "tests/frame.rs"]
mod tests;
