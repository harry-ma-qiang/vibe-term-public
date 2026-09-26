// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A viewer's keys, clicks, drags, scrolls and hovers applied to a session.

use crate::frame::HitZone;
use crate::session::Session;
use crate::status_bar::{self, HoverResult, StatusAction};
use std::io::Write;

impl Session {
    /// Apply one viewer input: forward keys, or handle a mouse event.
    pub fn handle_input(
        &mut self,
        action: crate::input::InputAction,
        out: &mut impl Write,
    ) -> std::io::Result<()> {
        use crate::input::InputAction;
        match action {
            InputAction::Forward(bytes) => {
                self.exit_all_scroll(out);
                if let Some(pane) = self.panes.get_mut(self.focused) {
                    match &mut pane.kind {
                        crate::pane::PaneKind::Pty(p) => {
                            let _ = p.write_input(&bytes);
                        }
                        crate::pane::PaneKind::Service { inner, .. } => {
                            inner.handle_input(crate::service::InputEvent::Bytes(bytes));
                        }
                    }
                }
            }
            InputAction::MouseClick { x, y } => {
                let now = std::time::Instant::now();
                let is_double = now.duration_since(self.last_click_time)
                    < std::time::Duration::from_millis(400)
                    && self.last_click_x == x
                    && self.last_click_y == y;
                self.last_click_time = now;
                self.last_click_x = x;
                self.last_click_y = y;
                let had_selection = self.clear_selection();
                let changed = if is_double {
                    self.handle_double_click(x as usize, y as usize)
                } else {
                    self.handle_click(x as usize, y as usize)
                };
                self.begin_selection(x as usize, y as usize);
                self.copy_visible_pane(out)?;
                if changed {
                    out.write_all(b"\x1b[2J")?;
                    self.render_all_borders(out)?;
                    self.render_status_bar(out)?;
                    self.force_render_all(out)?;
                } else if had_selection {
                    self.force_render_all(out)?;
                }
            }
            InputAction::MouseDrag { x, y } => {
                if self.extend_selection(x as usize, y as usize) {
                    self.render_selected_pane(out)?;
                }
            }
            InputAction::MouseUp { x, y } => {
                self.extend_selection(x as usize, y as usize);
                self.copy_selection(out)?;
            }
            InputAction::ScrollUp { x, y } => {
                self.handle_scroll(x as usize, y as usize, true, out)?;
            }
            InputAction::ScrollDown { x, y } => {
                self.handle_scroll(x as usize, y as usize, false, out)?;
            }
            InputAction::MouseMove { x, y } => match self.handle_hover(x as usize, y as usize) {
                HoverResult::StatusBar => self.render_status_bar(out)?,
                HoverResult::Borders => self.render_all_borders(out)?,
                HoverResult::None => {}
            },
            InputAction::None => {}
        }
        Ok(())
    }

    /// A single click: status bar button, pane close or copy, or focus change.
    pub fn handle_click(&mut self, x: usize, y: usize) -> bool {
        let layout = self.compute_layout();
        let r = layout.status_rect;
        if y >= r.y && y < r.y + r.h {
            return self.handle_status_click(x, y);
        }
        let mut to_close: Option<u32> = None;
        let mut focus_to: Option<usize> = None;
        for (i, pane) in self.panes.iter().enumerate() {
            match pane.frame.hit_test(x, y) {
                HitZone::CopyButton => {
                    self.copy_requested = Some(i);
                    return false;
                }
                HitZone::CloseButton => {
                    to_close = Some(pane.id);
                    break;
                }
                HitZone::Content => {
                    if self.focused != i {
                        focus_to = Some(i);
                    }
                    break;
                }
                _ => {}
            }
        }
        if let Some(id) = to_close {
            // why: a crashed service shows restart in the close slot; route the click there.
            if let Some(pane) = self.pane_by_id_mut(id)
                && pane.is_service_crashed()
            {
                pane.deliver_service_input(crate::service::InputEvent::Click {
                    x: x as u16,
                    y: y as u16,
                });
                return true;
            }
            return self.close_pane_if_idle(id);
        }
        if let Some(i) = focus_to {
            self.focused = i;
            return true;
        }
        false
    }

    /// A double click on a sidebar pane swaps it into the main slot.
    pub fn handle_double_click(&mut self, x: usize, y: usize) -> bool {
        for (i, pane) in self.panes.iter().enumerate() {
            if let HitZone::Content = pane.frame.hit_test(x, y)
                && i != 0
            {
                self.swap_with_main(i);
                return true;
            }
        }
        false
    }

    fn handle_status_click(&mut self, x: usize, y: usize) -> bool {
        let layout = self.compute_layout();
        let r = layout.status_rect;
        let on_line_2 = y == r.y + 1;
        if on_line_2 {
            return self.handle_status_click_line_2(x);
        }
        self.handle_status_click_line_1(x)
    }

    fn handle_status_click_line_1(&mut self, x: usize) -> bool {
        let bar = status_bar::build_text_line_1(self.filter_enabled);
        let positions = status_bar::button_positions_line_1(&bar);
        for (start, end, action) in positions {
            if x >= start && x <= end {
                let cancelled = self.confirm_quit_no();
                return self.dispatch_status_action(action) || cancelled;
            }
        }
        false
    }

    fn handle_status_click_line_2(&mut self, x: usize) -> bool {
        let (bridge_on, bridge_status, _bridge_err): (_, _, Option<String>) =
            (false, status_bar::BridgeStatusWord::Disabled, None);
        let bar = status_bar::build_text_line_2(
            bridge_on,
            bridge_status,
            self.quit_toast_active(),
            self.filter_enabled,
        );
        let positions = status_bar::button_positions_line_2(&bar);
        for (start, end, action) in positions {
            if x >= start && x <= end {
                return self.dispatch_status_action(action);
            }
        }
        self.confirm_quit_no()
    }

    fn dispatch_status_action(&mut self, action: StatusAction) -> bool {
        match action {
            StatusAction::AddPane => self.add_one_pane(),
            StatusAction::CloseIdle => !self.close_idle_panes().is_empty(),
            StatusAction::Split(r) => self.set_split_ratio(r),
            StatusAction::Filter => {
                if self.stage_mode {
                    return false;
                }
                self.toggle_filter();
                true
            }
            StatusAction::Detach => {
                self.detach_viewers = true;
                true
            }
            StatusAction::Quit => self.arm_quit_toast(),
            StatusAction::ToggleBridge => true,
            StatusAction::ConfirmQuitYes => self.confirm_quit_yes(),
            StatusAction::ConfirmQuitNo => self.confirm_quit_no(),
        }
    }

    /// Track the hovered button; says which part of the screen needs repainting.
    pub fn handle_hover(&mut self, x: usize, y: usize) -> HoverResult {
        let layout = self.compute_layout();
        let r = layout.status_rect;
        let was_on_bar = self.hover_y >= r.y && self.hover_y < r.y + r.h;
        let now_on_bar = y >= r.y && y < r.y + r.h;
        self.hover_x = x;
        self.hover_y = y;

        let old_close = self.hovered_close;
        let old_copy = self.hovered_copy;
        self.hovered_close = None;
        self.hovered_copy = None;
        for (i, pane) in self.panes.iter().enumerate() {
            match pane.frame.hit_test(x, y) {
                HitZone::CloseButton => self.hovered_close = Some(i),
                HitZone::CopyButton => self.hovered_copy = Some(i),
                _ => {}
            }
        }

        if old_close != self.hovered_close || old_copy != self.hovered_copy {
            HoverResult::Borders
        } else if was_on_bar || now_on_bar {
            HoverResult::StatusBar
        } else {
            HoverResult::None
        }
    }

    /// Scroll the pane under the cursor, or send arrows to an alternate-screen app.
    pub fn handle_scroll(
        &mut self,
        x: usize,
        y: usize,
        up: bool,
        out: &mut impl Write,
    ) -> std::io::Result<bool> {
        for pane in &mut self.panes {
            if let HitZone::Content = pane.frame.hit_test(x, y) {
                if pane.screen.is_alt_screen() {
                    let key: &[u8] = if up { b"\x1b[A" } else { b"\x1b[B" };
                    if let Some(pty) = pane.pty_opt_mut() {
                        for _ in 0..3 {
                            let _ = pty.write_input(key);
                        }
                    }
                    return Ok(true);
                }
                if up {
                    pane.scroll_up(3);
                } else {
                    pane.scroll_down(3);
                }
                pane.screen.mark_all_dirty();
                if pane.scroll_offset == 0 {
                    let _ = pane.render_full(out);
                } else {
                    let _ = pane.render(out);
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Return every scrolled-back pane to its live view.
    pub fn exit_all_scroll(&mut self, out: &mut impl Write) {
        let mut any = false;
        for pane in &mut self.panes {
            if pane.is_scrolled() {
                pane.exit_scroll();
                let _ = pane.render_full(out);
                any = true;
            }
        }
        if any {
            let _ = self.position_cursor(out);
        }
    }

    /// Whether any pane is scrolled back.
    pub fn any_scrolled(&self) -> bool {
        self.panes.iter().any(|p| p.is_scrolled())
    }

    /// Swap a sidebar pane into the main slot and focus it.
    pub fn swap_with_main(&mut self, sidebar_idx: usize) {
        if sidebar_idx == 0 || sidebar_idx >= self.panes.len() {
            return;
        }
        self.panes.swap(0, sidebar_idx);
        self.panes[0].frame.slot = 0;
        self.panes[sidebar_idx].frame.slot = sidebar_idx;
        self.focused = 0;
        self.refresh_layout();
    }
}
