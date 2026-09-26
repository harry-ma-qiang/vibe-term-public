// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Session lifecycle: the on-disk session directory and its config file,
//! resource sampling and limits, pane restarts, and orderly shutdown.

use crate::session::Session;
use std::io;
use std::path::Path;

impl Session {
    /// Create the session directory and write the initial config file.
    pub fn init_session_dir(&mut self, dir: &std::path::Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        self.session_dir = Some(dir.to_path_buf());
        self.save_config()
    }

    /// Write the session's layout and filter settings to its config file.
    pub fn save_config(&self) -> io::Result<()> {
        let dir = match &self.session_dir {
            Some(d) => d,
            None => return Ok(()),
        };
        let config = serde_json::json!({
            "name": self.name,
            "pid": std::process::id(),
            "sidebar_count": self.sidebar_count,
            "split_ratio": self.split_ratio.label(),
            "sidebar_side": match self.sidebar_side {
                crate::grid::SidebarSide::Left => "left",
                crate::grid::SidebarSide::Right => "right",
            },
            "filter_enabled": self.filter_enabled,
        });
        let path = dir.join("config.json");
        std::fs::write(
            path,
            serde_json::to_string_pretty(&config).unwrap_or_default(),
        )
    }

    /// Adopt a session directory, reading back any config file it holds.
    pub fn load_config(&mut self, dir: &Path) -> io::Result<()> {
        let path = dir.join("config.json");
        if !path.exists() {
            return Ok(());
        }
        let data = std::fs::read_to_string(&path)?;
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
            if let Some(s) = json["name"].as_str() {
                self.name = s.to_string();
            }
            if let Some(n) = json["sidebar_count"].as_u64() {
                self.sidebar_count = n as usize;
            }
            if let Some(s) = json["split_ratio"].as_str() {
                self.split_ratio = match s {
                    "1:4" => crate::grid::SplitRatio::OneToFour,
                    "1:1" => crate::grid::SplitRatio::OneToOne,
                    "4:1" => crate::grid::SplitRatio::FourToOne,
                    _ => crate::grid::SplitRatio::GoldenRatio,
                };
            }
            if let Some(s) = json["sidebar_side"].as_str() {
                self.sidebar_side = if s == "right" {
                    crate::grid::SidebarSide::Right
                } else {
                    crate::grid::SidebarSide::Left
                };
            }
            if let Some(b) = json["filter_enabled"].as_bool() {
                self.filter_enabled = b;
            }
        }
        self.session_dir = Some(dir.to_path_buf());
        Ok(())
    }

    /// Remove the session directory, for sessions marked as throwaway.
    pub fn cleanup_session_dir(&self) {
        if !self.clean_session {
            return;
        }
        if let Some(ref dir) = self.session_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Refresh every pane's cpu and memory numbers.
    pub fn sample_stats(&mut self) {
        // why: one process snapshot per tick; per-pane walks burst-blocked the loop.
        let map = crate::monitor::ProcMap::snapshot();
        for pane in &mut self.panes {
            let pid = pane.pty_opt().map(|p| p.child_pid.as_raw()).unwrap_or(0);
            if pid > 0 {
                pane.stats.sample_with_map(pid, &map);
            }
        }
    }

    /// Kill panes that broke a resource limit and queue them for restart.
    pub fn check_limits(&mut self) {
        let delay = std::time::Duration::from_secs(self.limits.restart_delay_secs);
        let mut to_kill = Vec::new();
        for (i, pane) in self.panes.iter().enumerate() {
            if self.limits.check(&pane.stats).is_some() {
                to_kill.push((i, pane.original_cmd.clone()));
            }
        }
        for (i, cmd) in to_kill.into_iter().rev() {
            if let Some(pane) = self.panes.get(i) {
                pane.kill_process_tree();
                self.restart_queue
                    .push((std::time::Instant::now() + delay, cmd));
            }
        }
    }

    /// Respawn queued commands whose restart delay has elapsed.
    pub fn process_restart_queue(&mut self) {
        let now = std::time::Instant::now();
        let mut ready = Vec::new();
        self.restart_queue.retain(|&(when, ref cmd)| {
            if now >= when {
                ready.push(cmd.clone());
                false
            } else {
                true
            }
        });
        for cmd in ready {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            let shell = if cfg!(target_os = "macos") {
                "zsh"
            } else {
                "bash"
            };
            let (scmd, sargs) = parts.split_first().unwrap_or((&shell, &[]));
            // why: validate argv before spawn; an unresolvable command reaches execvp.
            if crate::pty::validate_exec_cmd(scmd).is_err() {
                continue;
            }
            self.sidebar_count += 1;
            if let Ok(()) = self.add_sidebar_pane(scmd, sargs) {
                let _ = self.relayout();
            } else {
                self.sidebar_count -= 1;
            }
        }
    }

    /// True once no pane is left alive and the daemon may exit.
    pub fn should_exit(&self) -> bool {
        self.panes.is_empty() || self.panes.iter().all(|p| !p.is_alive())
    }

    /// Signal every pane's process group.
    pub fn kill_all(&self) {
        for pane in &self.panes {
            pane.kill_process_tree();
        }
    }

    /// The first non-zero child exit code, or zero when all succeeded.
    pub fn exit_code(&self) -> i32 {
        for pane in &self.panes {
            if let Some(pty) = pane.pty_opt()
                && let Ok(code) = pty.wait_exit()
                && code != 0
            {
                return code;
            }
        }
        0
    }
}
