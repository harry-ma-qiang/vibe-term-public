// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The quit-confirm toast: arming it, confirming or dismissing it, and the
//! window after which it dismisses itself.

use crate::session::Session;
use std::time::{Duration, Instant};

/// How long a quit click stays armed before the toast dismisses itself.
pub const QUIT_CONFIRM_WINDOW: Duration = Duration::from_secs(5);

impl Session {
    /// True while the quit-confirm toast should be drawn on the second row.
    pub fn quit_toast_active(&self) -> bool {
        match self.pending_quit_confirm {
            Some(t) => t.elapsed() < QUIT_CONFIRM_WINDOW,
            None => false,
        }
    }

    /// Drop an expired toast; true when state was cleared and a redraw is due.
    pub fn clear_expired_quit_confirm(&mut self) -> bool {
        match self.pending_quit_confirm {
            Some(t) if t.elapsed() >= QUIT_CONFIRM_WINDOW => {
                self.pending_quit_confirm = None;
                true
            }
            _ => false,
        }
    }

    /// Arm the toast; true so the caller triggers a redraw.
    pub fn arm_quit_toast(&mut self) -> bool {
        self.pending_quit_confirm = Some(Instant::now());
        true
    }

    /// Confirm the quit: clear the toast and request shutdown.
    pub fn confirm_quit_yes(&mut self) -> bool {
        self.pending_quit_confirm = None;
        self.shutdown_requested = true;
        true
    }

    /// Dismiss the toast without quitting; true when one was showing.
    pub fn confirm_quit_no(&mut self) -> bool {
        let was = self.pending_quit_confirm.is_some();
        self.pending_quit_confirm = None;
        was
    }
}

#[cfg(test)]
#[path = "tests/session_toolbar.rs"]
mod tests;
