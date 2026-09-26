// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Screen layout: one main pane, a stack of sidebar panes and the status bar,
//! with minimum sizes that collapse the sidebar away on a small terminal.

use crate::frame::Rect;

/// Which edge of the screen the sidebar stack occupies.
#[derive(Clone, Copy, PartialEq)]
pub enum SidebarSide {
    Left,
    Right,
}

/// Width split between the sidebar stack and the main pane.
#[derive(Clone, Copy, PartialEq)]
pub enum SplitRatio {
    OneToFour,
    GoldenRatio,
    OneToOne,
    FourToOne,
}

impl SplitRatio {
    /// Sidebar width in cells for a terminal of the given column count.
    pub fn sidebar_fraction(self, cols: usize) -> usize {
        match self {
            SplitRatio::OneToFour => cols / 5,
            SplitRatio::GoldenRatio => cols * 1000 / 2618,
            SplitRatio::OneToOne => cols / 2,
            SplitRatio::FourToOne => cols * 4 / 5,
        }
    }

    /// Short label shown on the status bar button.
    pub fn label(self) -> &'static str {
        match self {
            SplitRatio::OneToFour => "1:4",
            SplitRatio::GoldenRatio => "1:\u{03C6}",
            SplitRatio::OneToOne => "1:1",
            SplitRatio::FourToOne => "4:1",
        }
    }

    /// Every ratio, in the order the status bar draws them.
    pub const ALL: [SplitRatio; 4] = [
        SplitRatio::OneToFour,
        SplitRatio::GoldenRatio,
        SplitRatio::OneToOne,
        SplitRatio::FourToOne,
    ];
}

/// Computed rectangles for the main pane, the sidebar stack and the status bar.
pub struct GridLayout {
    pub main_rect: Rect,
    pub sidebar_rects: Vec<Rect>,
    pub status_rect: Rect,
    pub sidebar_count: usize,
    pub sidebar_side: SidebarSide,
}

// why: two rows, buttons on line 1, bridge widget and toast on line 2.
const STATUS_BAR_HEIGHT: usize = 2;
const MIN_MAIN_WIDTH: usize = 20;
const MIN_SIDEBAR_WIDTH: usize = 10;
const MIN_PANE_HEIGHT: usize = 3;

/// Terminal size and sidebar preferences a layout is computed from.
pub struct GridConfig {
    pub cols: usize,
    pub rows: usize,
    pub sidebar_count: usize,
    pub sidebar_side: SidebarSide,
    pub split_ratio: SplitRatio,
}

impl GridLayout {
    /// Lay out the screen, collapsing the sidebar when the terminal is small.
    pub fn compute(cfg: &GridConfig) -> Self {
        let cols = cfg.cols;
        let rows = cfg.rows;
        let sidebar_count = cfg.sidebar_count;
        let sidebar_side = cfg.sidebar_side;
        let split_ratio = cfg.split_ratio;
        let status_h = STATUS_BAR_HEIGHT;
        let grid_h = rows.saturating_sub(status_h);
        let remainder = cols;

        if sidebar_count == 0
            || remainder < MIN_MAIN_WIDTH + MIN_SIDEBAR_WIDTH
            || grid_h < MIN_PANE_HEIGHT
        {
            return Self {
                main_rect: Rect {
                    x: 0,
                    y: 0,
                    w: remainder,
                    h: grid_h,
                },
                sidebar_rects: Vec::new(),
                status_rect: Rect {
                    x: 0,
                    y: grid_h,
                    w: cols,
                    h: status_h,
                },
                sidebar_count: 0,
                sidebar_side,
            };
        }

        let sidebar_w = split_ratio
            .sidebar_fraction(remainder)
            .max(MIN_SIDEBAR_WIDTH);
        let main_w = remainder.saturating_sub(sidebar_w);

        if main_w < MIN_MAIN_WIDTH {
            return Self {
                main_rect: Rect {
                    x: 0,
                    y: 0,
                    w: remainder,
                    h: grid_h,
                },
                sidebar_rects: Vec::new(),
                status_rect: Rect {
                    x: 0,
                    y: grid_h,
                    w: cols,
                    h: status_h,
                },
                sidebar_count: 0,
                sidebar_side,
            };
        }

        let effective_count = fit_sidebar_count(sidebar_count, grid_h);

        let (side_x, main_x) = match sidebar_side {
            SidebarSide::Left => (0, sidebar_w),
            SidebarSide::Right => (main_w, 0),
        };

        let main_rect = Rect {
            x: main_x,
            y: 0,
            w: main_w,
            h: grid_h,
        };

        let mut sidebar_rects = Vec::with_capacity(effective_count);
        for i in 0..effective_count {
            let pane_y = i * grid_h / effective_count;
            let next_y = (i + 1) * grid_h / effective_count;
            let pane_h = next_y - pane_y;
            sidebar_rects.push(Rect {
                x: side_x,
                y: pane_y,
                w: sidebar_w,
                h: pane_h,
            });
        }

        let status_rect = Rect {
            x: 0,
            y: grid_h,
            w: cols,
            h: status_h,
        };

        Self {
            main_rect,
            sidebar_rects,
            status_rect,
            sidebar_count: effective_count,
            sidebar_side,
        }
    }
}

fn fit_sidebar_count(requested: usize, grid_h: usize) -> usize {
    let mut count = requested;
    while count > 0 && grid_h / count < MIN_PANE_HEIGHT {
        count /= 2;
    }
    count.max(1)
}

#[cfg(test)]
#[path = "tests/grid.rs"]
mod tests;
