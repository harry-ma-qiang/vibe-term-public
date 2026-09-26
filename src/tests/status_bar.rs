// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the status bar: button text on both rows, the column spans a
//! click maps through, and what the painted rows actually contain.

use crate::status_bar::{
    BridgeStatusWord, StatusAction, build_text_line_1, build_text_line_2, button_positions_line_1,
    button_positions_line_2,
};

#[test]
fn line_1_has_short_close_button_and_no_lock_chip() {
    let bar = build_text_line_1(false);
    assert!(bar.contains("[+]"), "missing [+]: {bar}");
    assert!(bar.contains("[x]"), "missing [x]: {bar}");
    assert!(
        !bar.contains("[0x]"),
        "[0x] should be renamed to [x]: {bar}"
    );
    assert!(
        !bar.contains("L:"),
        "lock chip must NOT appear on line 1: {bar}"
    );
}

#[test]
fn line_1_x_button_is_close_idle_action() {
    let bar = build_text_line_1(false);
    let positions = button_positions_line_1(&bar);
    assert!(
        matches!(positions[0].2, StatusAction::AddPane),
        "first button is [+]"
    );
    assert!(
        matches!(positions[1].2, StatusAction::CloseIdle),
        "second button must remain CloseIdle"
    );
}

#[test]
fn x_button_hit_zone_shifts_left_by_one_cell_after_rename() {
    let renamed = build_text_line_1(false);
    let mut legacy = String::new();
    legacy.push_str("Panes: [+] [0x] \u{2502} Split: ");
    for ratio in crate::grid::SplitRatio::ALL {
        legacy.push_str(&format!("[{}] ", ratio.label()));
    }
    legacy.push_str("\u{2502} [D] [Q]");

    let pos_new = button_positions_line_1(&renamed);
    let pos_old = button_positions_line_1(&legacy);
    let q_new = pos_new
        .iter()
        .find(|(_, _, a)| matches!(a, StatusAction::Quit))
        .expect("renamed [Q]");
    let q_old = pos_old
        .iter()
        .find(|(_, _, a)| matches!(a, StatusAction::Quit))
        .expect("legacy [Q]");
    assert_eq!(
        q_new.0,
        q_old.0 - 1,
        "[Q] must shift left by 1 cell after [0x] -> [x] rename"
    );
}

#[test]
fn line_1_has_detach_quit_in_order_no_filter() {
    let bar = build_text_line_1(false);
    let positions = button_positions_line_1(&bar);
    assert!(
        !positions
            .iter()
            .any(|(_, _, a)| matches!(a, StatusAction::Filter)),
        "Filter moved to line 2, must NOT appear on line 1"
    );
    let detach = positions
        .iter()
        .position(|(_, _, a)| matches!(a, StatusAction::Detach))
        .expect("Detach present");
    let quit = positions
        .iter()
        .position(|(_, _, a)| matches!(a, StatusAction::Quit))
        .expect("Quit present");
    assert!(detach < quit);
}

#[test]
fn line_2_default_shows_bridge_off_disabled() {
    let bar = build_text_line_2(false, BridgeStatusWord::Disabled, false, false);
    assert!(bar.contains("[W:OFF]"), "default line 2 has [W:OFF]: {bar}");
    assert!(bar.contains("disabled"), "status word reads 'disabled'");
    assert!(
        !bar.contains("Quit all sessions?"),
        "no toast by default: {bar}"
    );
}

#[test]
fn line_2_w_on_shows_status_word() {
    let bar = build_text_line_2(true, BridgeStatusWord::Connected, false, false);
    assert!(bar.contains("[W:ON"));
    assert!(bar.contains("connected"));
}

#[test]
fn line_2_w_off_click_routes_to_toggle_bridge() {
    let bar = build_text_line_2(false, BridgeStatusWord::Disabled, false, false);
    let positions = button_positions_line_2(&bar);
    let w = positions
        .iter()
        .find(|(_, _, a)| matches!(a, StatusAction::ToggleBridge))
        .expect("[W:OFF] is clickable");
    assert!(w.0 > 2, "[W:OFF] shifted right by [Bot:OFF] button width");
}

#[test]
fn line_2_w_on_click_also_routes_to_toggle_bridge() {
    let bar = build_text_line_2(true, BridgeStatusWord::Connected, false, false);
    let positions = button_positions_line_2(&bar);
    let count = positions
        .iter()
        .filter(|(_, _, a)| matches!(a, StatusAction::ToggleBridge))
        .count();
    assert_eq!(count, 1, "[W:ON] is the toggle button");
}

#[test]
fn line_2_quit_toast_appends_y_n_buttons() {
    let bar = build_text_line_2(false, BridgeStatusWord::Disabled, true, false);
    assert!(
        bar.contains("Quit all sessions?"),
        "toast text present: {bar}"
    );
    assert!(bar.contains("[Y]"), "[Y] button present: {bar}");
    assert!(bar.contains("[N]"), "[N] button present: {bar}");
    let positions = button_positions_line_2(&bar);
    assert!(
        positions
            .iter()
            .any(|(_, _, a)| matches!(a, StatusAction::ConfirmQuitYes)),
        "[Y] routes to ConfirmQuitYes"
    );
    assert!(
        positions
            .iter()
            .any(|(_, _, a)| matches!(a, StatusAction::ConfirmQuitNo)),
        "[N] routes to ConfirmQuitNo"
    );
}

#[test]
fn status_bar_visual_snapshot() {
    use crate::emulator::Screen;
    use crate::grid::SplitRatio;
    use crate::status_bar::{BridgeStatusWord, RenderArgs, render};

    let args = RenderArgs {
        area: crate::frame::Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 2,
        },
        hover_x: 0,
        hover_y: 0,
        hover_on_bar: false,
        sidebar_count: 0,
        split_ratio: SplitRatio::OneToOne,
        filter_enabled: false,
        stage_mode: false,
        headless: false,
        active_children: 0,
        bridge_on: true,
        bridge_status: BridgeStatusWord::Connected,
        quit_pending: false,
        bridge_error: None,
    };

    let mut out = Vec::new();
    render(&mut out, &args).unwrap();

    let mut screen = Screen::new(80, 2);
    screen.feed(&out);

    let row0 = screen.row_text(0);
    let row1 = screen.row_text(1);

    assert!(
        row0.contains("Panes:"),
        "row0 should contain 'Panes', got: '{}'",
        row0
    );
    assert!(
        row0.contains("Split:"),
        "row0 should contain 'Split', got: '{}'",
        row0
    );
    assert!(
        row1.contains("[W:ON"),
        "row1 should contain '[W:ON', got: '{}'",
        row1
    );
    assert!(
        row1.contains("[F:OFF]"),
        "row1 should contain '[F:OFF]', got: '{}'",
        row1
    );
}
