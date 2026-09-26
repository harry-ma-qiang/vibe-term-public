// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Pane-local mouse selection: anchor and cursor tracking, hit testing, the
//! selected text pulled back out of a screen, and clipboard delivery.

use crate::emulator::Screen;
use crate::frame::Rect;

/// An in-progress or finished selection inside one pane's content area.
#[derive(Clone, Copy, PartialEq)]
pub struct Selection {
    pub pane_idx: usize,
    pub scroll_offset: usize,
    anchor: (usize, usize),
    cursor: (usize, usize),
}

/// Fold absolute terminal coordinates into a rectangle's own cell grid.
///
/// A drag leaving the pane can never address a border or a neighbour.
pub fn clamp_to(rect: &Rect, x: usize, y: usize) -> (usize, usize) {
    let col = x.saturating_sub(rect.x).min(rect.w.saturating_sub(1));
    let row = y.saturating_sub(rect.y).min(rect.h.saturating_sub(1));
    (col, row)
}

impl Selection {
    /// Begin a selection anchored at one cell of a pane.
    pub fn start(pane_idx: usize, scroll_offset: usize, col: usize, row: usize) -> Self {
        Self {
            pane_idx,
            scroll_offset,
            anchor: (col, row),
            cursor: (col, row),
        }
    }

    /// Move the free end of the selection to a new cell.
    pub fn extend_to(&mut self, col: usize, row: usize) {
        self.cursor = (col, row);
    }

    /// True when nothing is selected, as after a plain click.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }

    /// The two ends in reading order, regardless of drag direction.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        let (ac, ar) = self.anchor;
        let (cc, cr) = self.cursor;
        if (ar, ac) <= (cr, cc) {
            ((ac, ar), (cc, cr))
        } else {
            ((cc, cr), (ac, ar))
        }
    }

    /// True when a cell falls inside the selected span.
    pub fn contains(&self, col: usize, row: usize) -> bool {
        let ((sc, sr), (ec, er)) = self.ordered();
        if row < sr || row > er {
            return false;
        }
        if sr == er {
            return col >= sc && col <= ec;
        }
        if row == sr {
            return col >= sc;
        }
        if row == er {
            return col <= ec;
        }
        true
    }

    /// The selected text, with soft-wrapped rows rejoined into one line.
    pub fn text(&self, screen: &Screen) -> String {
        let ((sc, sr), (ec, er)) = self.ordered();
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for row in sr..=er {
            let chars: Vec<char> = visible_row_text(screen, self.scroll_offset, row)
                .chars()
                .collect();
            let from = if row == sr { sc } else { 0 };
            let to = if row == er {
                ec
            } else {
                chars.len().saturating_sub(1)
            };
            let slice: String = if chars.is_empty() || from >= chars.len() || from > to {
                String::new()
            } else {
                chars[from..=to.min(chars.len() - 1)].iter().collect()
            };
            // why: a soft-wrapped row is one logical line; a newline here would be fake.
            if row < er && row_is_wrapped(screen, self.scroll_offset, row) {
                current.push_str(&slice);
            } else {
                current.push_str(slice.trim_end());
                lines.push(std::mem::take(&mut current));
            }
        }
        lines.join("\n")
    }
}

fn row_is_wrapped(screen: &Screen, scroll_offset: usize, row: usize) -> bool {
    if scroll_offset == 0 {
        return screen.wrapped.get(row).copied().unwrap_or(false);
    }
    let sb_len = screen.scrollback_len();
    let view_start = sb_len.saturating_sub(scroll_offset);
    let abs = view_start + row;
    if abs < sb_len {
        screen.scrollback_is_wrapped(abs)
    } else {
        screen.wrapped.get(abs - sb_len).copied().unwrap_or(false)
    }
}

fn visible_row_text(screen: &Screen, scroll_offset: usize, row: usize) -> String {
    if scroll_offset == 0 {
        return screen.row_text(row);
    }
    let sb_len = screen.scrollback_len();
    let view_start = sb_len.saturating_sub(scroll_offset);
    let abs = view_start + row;
    if abs < sb_len {
        screen.scrollback_row_text(abs).unwrap_or_default()
    } else {
        let screen_row = abs - sb_len;
        if screen_row < screen.rows {
            screen.row_text(screen_row)
        } else {
            String::new()
        }
    }
}

// why: OSC 52 survives ssh, but some terminals ignore it, so try a helper first.
/// Encode text as an OSC 52 clipboard escape sequence.
pub fn osc52(text: &str) -> Vec<u8> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as B64;
    format!("\x1b]52;c;{}\x07", B64.encode(text.as_bytes())).into_bytes()
}

const CLIPBOARD_HELPERS: &[(&str, &[&str])] = &[
    ("pbcopy", &[]),
    ("wl-copy", &[]),
    ("xclip", &["-selection", "clipboard"]),
    ("xsel", &["--clipboard", "--input"]),
];

/// Try each host clipboard helper in turn; true when one accepted the text.
pub fn copy_to_host_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for (bin, args) in CLIPBOARD_HELPERS {
        let Ok(mut child) = Command::new(bin)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        let wrote = child
            .stdin
            .take()
            .map(|mut s| s.write_all(text.as_bytes()).is_ok())
            .unwrap_or(false);
        if child.wait().map(|s| s.success()).unwrap_or(false) && wrote {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "tests/selection.rs"]
mod tests;
