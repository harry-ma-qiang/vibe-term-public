// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for screen layout: sidebar sides and counts, split ratios, minimum
//! pane sizes and the status bar always sitting at the bottom.

use super::*;

fn compute_default(cols: usize, rows: usize, count: usize) -> GridLayout {
    GridLayout::compute(&GridConfig {
        cols,
        rows,
        sidebar_count: count,
        sidebar_side: SidebarSide::Left,
        split_ratio: SplitRatio::GoldenRatio,
    })
}

fn compute_cfg(
    cols: usize,
    rows: usize,
    count: usize,
    side: SidebarSide,
    ratio: SplitRatio,
) -> GridLayout {
    GridLayout::compute(&GridConfig {
        cols,
        rows,
        sidebar_count: count,
        sidebar_side: side,
        split_ratio: ratio,
    })
}

#[test]
fn fullscreen_when_sidebar_zero() {
    let g = compute_default(160, 45, 0);
    assert_eq!(g.sidebar_rects.len(), 0);
    assert_eq!(g.main_rect.x, 0);
    assert_eq!(g.main_rect.w, 160);
    assert_eq!(g.main_rect.h, 43);
    assert_eq!(g.status_rect.y, 43);
    assert_eq!(g.status_rect.h, 2);
}

#[test]
fn sidebar_left_1x() {
    let g = compute_default(160, 45, 1);
    assert_eq!(g.sidebar_rects.len(), 1);
    let side = &g.sidebar_rects[0];
    assert_eq!(side.x, 0);
    assert!(side.w > 0);
    assert_eq!(g.main_rect.x, side.w);
    assert_eq!(g.main_rect.w + side.w, 160);
    assert_eq!(g.main_rect.h, 43);
}

#[test]
fn sidebar_right_1x() {
    let g = compute_cfg(160, 45, 1, SidebarSide::Right, SplitRatio::GoldenRatio);
    assert_eq!(g.sidebar_rects.len(), 1);
    let side = &g.sidebar_rects[0];
    assert_eq!(g.main_rect.x, 0);
    assert_eq!(side.x, g.main_rect.w);
    assert_eq!(g.main_rect.w + side.w, 160);
}

#[test]
fn sidebar_2x_equal_heights() {
    let g = compute_default(160, 45, 2);
    assert_eq!(g.sidebar_rects.len(), 2);
    let h0 = g.sidebar_rects[0].h;
    let h1 = g.sidebar_rects[1].h;
    assert!(
        (h0 as i32 - h1 as i32).unsigned_abs() <= 1,
        "heights should be ~equal: {h0} vs {h1}"
    );
    assert_eq!(h0 + h1, 43);
}

#[test]
fn sidebar_4x_equal_heights() {
    let g = compute_default(160, 45, 4);
    assert_eq!(g.sidebar_rects.len(), 4);
    let total_h: usize = g.sidebar_rects.iter().map(|r| r.h).sum();
    assert_eq!(total_h, 43);
    for r in &g.sidebar_rects {
        assert!(r.h >= MIN_PANE_HEIGHT, "pane too short: {}", r.h);
    }
}

#[test]
fn sidebar_8x_on_large_terminal() {
    let g = compute_default(200, 80, 8);
    assert_eq!(g.sidebar_rects.len(), 8);
    let total_h: usize = g.sidebar_rects.iter().map(|r| r.h).sum();
    assert_eq!(total_h, 78);
}

#[test]
fn sidebar_8x_reduced_on_small_terminal() {
    let g = compute_default(160, 20, 8);
    assert!(
        g.sidebar_count < 8,
        "should reduce from 8: got {}",
        g.sidebar_count
    );
    for r in &g.sidebar_rects {
        assert!(r.h >= MIN_PANE_HEIGHT, "pane too short: {}", r.h);
    }
}

#[test]
fn tiny_terminal_goes_fullscreen() {
    let g = compute_default(25, 10, 4);
    assert_eq!(g.sidebar_count, 0);
    assert_eq!(g.main_rect.w, 25);
}

#[test]
fn sidebar_rects_no_gaps_no_overlap() {
    let g = compute_default(160, 45, 4);
    for i in 1..g.sidebar_rects.len() {
        let prev_end = g.sidebar_rects[i - 1].y + g.sidebar_rects[i - 1].h;
        assert_eq!(
            prev_end, g.sidebar_rects[i].y,
            "gap or overlap at sidebar {i}"
        );
    }
}

#[test]
fn all_sidebar_panes_same_x_and_width() {
    let g = compute_default(160, 45, 4);
    for r in &g.sidebar_rects {
        assert_eq!(r.x, 0);
        assert_eq!(r.w, g.sidebar_rects[0].w);
    }
}

#[test]
fn status_bar_always_at_bottom() {
    for count in [0, 1, 2, 4, 8] {
        let g = compute_default(160, 45, count);
        assert_eq!(
            g.status_rect.y + g.status_rect.h,
            45,
            "status bar not at bottom for count={count}"
        );
    }
}

#[test]
fn main_plus_sidebar_fill_width() {
    let g = compute_default(160, 45, 2);
    let side_w = g.sidebar_rects[0].w;
    assert_eq!(g.main_rect.w + side_w, 160);
}

#[test]
fn golden_ratio_approximately_38_percent() {
    let g = compute_cfg(160, 45, 1, SidebarSide::Left, SplitRatio::GoldenRatio);
    let side_w = g.sidebar_rects[0].w;
    let pct = side_w * 100 / 160;
    assert!(
        (35..=42).contains(&pct),
        "golden ratio sidebar should be ~38%: got {pct}%"
    );
}

#[test]
fn one_to_four_ratio_20_percent() {
    let g = compute_cfg(160, 45, 1, SidebarSide::Left, SplitRatio::OneToFour);
    let side_w = g.sidebar_rects[0].w;
    assert_eq!(side_w, 32);
}

#[test]
fn one_to_one_ratio_50_percent() {
    let g = compute_cfg(160, 45, 1, SidebarSide::Left, SplitRatio::OneToOne);
    let side_w = g.sidebar_rects[0].w;
    assert_eq!(side_w, 80);
    assert_eq!(g.main_rect.w, 80);
}

#[test]
fn four_to_one_ratio_80_percent() {
    let g = compute_cfg(160, 45, 1, SidebarSide::Left, SplitRatio::FourToOne);
    let side_w = g.sidebar_rects[0].w;
    assert_eq!(side_w, 128);
    assert_eq!(g.main_rect.w, 32);
}

#[test]
fn all_ratios_fill_width() {
    for ratio in SplitRatio::ALL {
        let g = compute_cfg(160, 45, 1, SidebarSide::Left, ratio);
        if g.sidebar_rects.is_empty() {
            continue;
        }
        let total = g.main_rect.w + g.sidebar_rects[0].w;
        assert_eq!(
            total,
            160,
            "ratio {:?} doesn't fill width: {total}",
            ratio.label()
        );
    }
}
