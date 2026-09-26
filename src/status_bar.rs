// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The two-row status bar: button text, hover highlighting, painting and the
//! mapping from a clicked column back to the action the button stands for.

use unicode_width::UnicodeWidthStr;

use crate::frame::Rect;
use crate::grid::SplitRatio;
use std::io::{self, Write};

/// Rows the status bar occupies at the bottom of the screen.
pub const STATUS_BAR_ROWS: usize = 2;

/// Everything the status bar needs to paint its two rows.
pub struct RenderArgs {
    pub area: Rect,
    pub hover_x: usize,
    pub hover_y: usize,
    pub hover_on_bar: bool,
    pub sidebar_count: usize,
    pub split_ratio: SplitRatio,
    pub filter_enabled: bool,
    pub stage_mode: bool,
    pub headless: bool,
    pub active_children: usize,
    pub bridge_on: bool,
    pub bridge_status: BridgeStatusWord,
    pub quit_pending: bool,
    pub bridge_error: Option<String>,
}

/// Connection word shown beside the bridge toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeStatusWord {
    Disabled,
    Waiting,
    Connected,
    Error,
}

impl BridgeStatusWord {
    /// The word as it is written on the bar.
    pub fn as_str(self) -> &'static str {
        match self {
            BridgeStatusWord::Disabled => "disabled",
            BridgeStatusWord::Waiting => "waiting",
            BridgeStatusWord::Connected => "connected",
            BridgeStatusWord::Error => "error",
        }
    }
}

/// What the pointer is currently over.
pub enum HoverResult {
    None,
    StatusBar,
    Borders,
}

/// Action a click on a status bar button stands for.
pub enum StatusAction {
    AddPane,
    CloseIdle,
    Split(SplitRatio),
    Filter,
    Detach,
    Quit,
    ToggleBridge,
    ConfirmQuitYes,
    ConfirmQuitNo,
}

/// Line 1 text for the given filter and headless state.
pub fn build_text(filter_enabled: bool, headless: bool) -> String {
    build_text_with_lock(filter_enabled, headless, None)
}

/// Line 1 text; the headless and lock arguments are kept for older callers.
pub fn build_text_with_lock(
    filter_enabled: bool,
    _headless: bool,
    _lock_label: Option<&str>,
) -> String {
    build_text_line_1(filter_enabled)
}

/// Line 1: pane buttons, the four split ratios, detach and quit.
/// Toggle labels are fixed width so button positions never drift.
pub fn build_text_line_1(_filter_enabled: bool) -> String {
    let mut bar = String::new();
    bar.push_str("Panes: [+] [x] \u{2502} Split: ");
    for ratio in SplitRatio::ALL {
        bar.push_str(&format!("[{}] ", ratio.label()));
    }
    bar.push_str("\u{2502} [D] [Q]");
    bar
}

/// Line 2: filter and bridge toggles, the status word, and a quit toast.
pub fn build_text_line_2(
    bridge_on: bool,
    bridge_status: BridgeStatusWord,
    quit_pending: bool,
    filter_enabled: bool,
) -> String {
    let mut bar = String::new();
    let filter_label = if filter_enabled { "F:ON " } else { "F:OFF" };
    bar.push_str(&format!("[{filter_label}] "));
    // why: pad "W:ON " so that [W:OFF] and [W:ON] occupy the same width.
    let w_label = if bridge_on { "W:ON " } else { "W:OFF" };
    let status_word = if bridge_on {
        bridge_status.as_str()
    } else {
        BridgeStatusWord::Disabled.as_str()
    };
    bar.push_str(&format!("[{w_label}] {status_word}"));
    if quit_pending {
        bar.push_str(" \u{2502} Quit all sessions? [Y] [N]");
    }
    bar
}

/// Line 2 followed by a red error toast when the bridge reports one.
pub fn build_text_line_2_with_error(
    bridge_on: bool,
    bridge_status: BridgeStatusWord,
    quit_pending: bool,
    filter_enabled: bool,
    bridge_error: Option<&str>,
) -> String {
    let mut bar = build_text_line_2(bridge_on, bridge_status, quit_pending, filter_enabled);
    if let Some(err) = bridge_error {
        bar.push_str(&format!(" \u{2502} \x1b[1;31m{}\x1b[0m", err));
    }
    bar
}

/// Paint both status bar rows, then flush the writer.
pub fn render(out: &mut impl Write, args: &RenderArgs) -> io::Result<()> {
    render_line_1(out, args)?;
    render_line_2(out, args)?;
    out.flush()
}

fn render_line_1(out: &mut impl Write, args: &RenderArgs) -> io::Result<()> {
    let bar_text = build_text_line_1(args.filter_enabled);
    let line_y = args.area.y;
    let hover_active = args.hover_on_bar && args.hover_y == line_y;
    let positions = if hover_active {
        button_positions_line_1(&bar_text)
    } else {
        Vec::new()
    };
    let hovered = positions
        .iter()
        .position(|(start, end, _)| args.hover_x >= *start && args.hover_x <= *end);

    write!(out, "\x1b[{};{}H", line_y + 1, args.area.x + 1)?;
    write!(out, "\x1b[90mPanes: ")?;
    let mut btn_idx = 0;
    write_btn(out, hovered == Some(btn_idx), "[+]", "32")?;
    btn_idx += 1;
    write!(out, " ")?;
    let _close_idle_disabled =
        args.active_children == args.sidebar_count + 1 || args.sidebar_count == 0;
    write_btn(out, hovered == Some(btn_idx), "[x]", "33")?;
    btn_idx += 1;
    write!(out, " \x1b[90m\u{2502} Split: ")?;
    for ratio in SplitRatio::ALL {
        let label = ratio.label();
        let is_hovered = hovered == Some(btn_idx);
        if is_hovered {
            write!(out, "\x1b[7;37m[{label}]\x1b[0;90m ")?;
        } else if ratio == args.split_ratio {
            write!(out, "\x1b[1;36m[{label}]\x1b[90m ")?;
        } else {
            write!(out, "\x1b[37m[{label}]\x1b[90m ")?;
        }
        btn_idx += 1;
    }
    write!(out, "\x1b[90m\u{2502} ")?;
    write_btn(out, hovered == Some(btn_idx), "[D]", "36")?;
    btn_idx += 1;
    write!(out, " ")?;
    write_btn(out, hovered == Some(btn_idx), "[Q]", "31")?;

    let content_width = UnicodeWidthStr::width(bar_text.as_str());
    let remaining = args.area.w.saturating_sub(content_width);
    for _ in 0..remaining {
        write!(out, " ")?;
    }
    write!(out, "\x1b[0m")
}

fn render_line_2(out: &mut impl Write, args: &RenderArgs) -> io::Result<()> {
    let line_y = args.area.y + 1;
    if line_y >= args.area.y + args.area.h {
        return Ok(());
    }
    let bar_text = build_text_line_2_with_error(
        args.bridge_on,
        args.bridge_status,
        args.quit_pending,
        args.filter_enabled,
        args.bridge_error.as_deref(),
    );
    let hover_active = args.hover_on_bar && args.hover_y == line_y;
    let positions = if hover_active {
        button_positions_line_2(&bar_text)
    } else {
        Vec::new()
    };
    let hovered = positions
        .iter()
        .position(|(start, end, _)| args.hover_x >= *start && args.hover_x <= *end);

    write!(out, "\x1b[{};{}H", line_y + 1, args.area.x + 1)?;
    let mut btn_idx = 0;
    let filter_label = if args.filter_enabled {
        "F:ON "
    } else {
        "F:OFF"
    };
    let filter_color = if args.filter_enabled { "33" } else { "90" };
    write_btn(
        out,
        hovered == Some(btn_idx),
        &format!("[{filter_label}]"),
        filter_color,
    )?;
    btn_idx += 1;
    write!(out, " ")?;
    let w_label = if args.bridge_on { "W:ON " } else { "W:OFF" };
    let w_color = if args.bridge_on { "32" } else { "31" };
    write_btn(
        out,
        hovered == Some(btn_idx),
        &format!("[{w_label}]"),
        w_color,
    )?;
    btn_idx += 1;
    let status_word = if args.bridge_on {
        args.bridge_status.as_str()
    } else {
        BridgeStatusWord::Disabled.as_str()
    };
    write!(out, " \x1b[90m{status_word}\x1b[0m")?;
    if args.quit_pending {
        write!(
            out,
            " \x1b[90m\u{2502}\x1b[0m \x1b[1;33mQuit all sessions?\x1b[0m "
        )?;
        write_btn(out, hovered == Some(btn_idx), "[Y]", "32")?;
        btn_idx += 1;
        write!(out, " ")?;
        write_btn(out, hovered == Some(btn_idx), "[N]", "31")?;
    }
    let content_width = UnicodeWidthStr::width(bar_text.as_str());
    let remaining = args.area.w.saturating_sub(content_width);
    for _ in 0..remaining {
        write!(out, " ")?;
    }
    write!(out, "\x1b[0m")
}

fn write_btn(out: &mut impl Write, hovered: bool, label: &str, color: &str) -> io::Result<()> {
    if hovered {
        write!(out, "\x1b[7;{color}m{label}\x1b[0;90m")
    } else {
        write!(out, "\x1b[{color}m{label}\x1b[90m")
    }
}

/// Column spans and actions for the line 1 buttons.
/// Spans are measured by display width, so a label rename shifts the rest.
pub fn button_positions_line_1(bar: &str) -> Vec<(usize, usize, StatusAction)> {
    let mut positions = Vec::new();
    let mut col = 0;
    let mut split_idx = 0;
    let mut in_bracket = false;
    let mut bracket_start = 0;
    let mut bracket_text = String::new();

    for ch in bar.chars() {
        let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0u8; 4]) as &str);
        if ch == '[' {
            in_bracket = true;
            bracket_start = col;
            bracket_text.clear();
        } else if ch == ']' && in_bracket {
            in_bracket = false;
            let end = col;
            if bracket_text == "+" {
                positions.push((bracket_start, end, StatusAction::AddPane));
            } else if bracket_text == "x" {
                positions.push((bracket_start, end, StatusAction::CloseIdle));
            } else if bracket_text.contains(':') {
                if split_idx < SplitRatio::ALL.len() {
                    positions.push((
                        bracket_start,
                        end,
                        StatusAction::Split(SplitRatio::ALL[split_idx]),
                    ));
                    split_idx += 1;
                }
            } else if bracket_text == "D" {
                positions.push((bracket_start, end, StatusAction::Detach));
            } else if bracket_text == "Q" {
                positions.push((bracket_start, end, StatusAction::Quit));
            }
        } else if in_bracket {
            bracket_text.push(ch);
        }
        col += w;
    }
    positions
}

/// Column spans and actions for the line 2 buttons.
/// A bracketed label with no action, such as the lock chip, is skipped.
pub fn button_positions_line_2(bar: &str) -> Vec<(usize, usize, StatusAction)> {
    let mut positions = Vec::new();
    let mut col = 0;
    let mut in_bracket = false;
    let mut bracket_start = 0;
    let mut bracket_text = String::new();

    for ch in bar.chars() {
        let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0u8; 4]) as &str);
        if ch == '[' {
            in_bracket = true;
            bracket_start = col;
            bracket_text.clear();
        } else if ch == ']' && in_bracket {
            in_bracket = false;
            let end = col;
            if bracket_text.trim() == "F:ON" || bracket_text.trim() == "F:OFF" {
                positions.push((bracket_start, end, StatusAction::Filter));
            } else if bracket_text.trim() == "W:ON" || bracket_text.trim() == "W:OFF" {
                positions.push((bracket_start, end, StatusAction::ToggleBridge));
            } else if bracket_text == "Y" {
                positions.push((bracket_start, end, StatusAction::ConfirmQuitYes));
            } else if bracket_text == "N" {
                positions.push((bracket_start, end, StatusAction::ConfirmQuitNo));
            }
        } else if in_bracket {
            bracket_text.push(ch);
        }
        col += w;
    }
    positions
}

/// Line 1 button spans, for callers that predate the second row.
pub fn button_positions(bar: &str) -> Vec<(usize, usize, StatusAction)> {
    button_positions_line_1(bar)
}

#[cfg(test)]
#[path = "tests/status_bar.rs"]
mod tests;
