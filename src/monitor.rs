// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Per-pane resource sampling: cpu and memory for a process and its whole
//! descendant tree, plus the limits that decide when a pane is over budget.

use std::collections::HashMap;
use std::time::Instant;

/// Cpu percentage and resident memory for one pane's process tree.
#[derive(Clone, Default)]
pub struct PaneStats {
    pub cpu_percent: f32,
    pub mem_mb: f32,
    prev_ticks: u64,
    prev_time: Option<Instant>,
}

/// Parent-to-children map for every process, built once per sample tick.
pub struct ProcMap {
    children_of: HashMap<i32, Vec<i32>>,
}

impl ProcMap {
    /// Read every process's parent link from `/proc` into one map.
    #[cfg(target_os = "linux")]
    pub fn snapshot() -> Self {
        // why: one map per tick; reading /proc per child blocked the event loop.
        let mut children_of: HashMap<i32, Vec<i32>> = HashMap::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Self { children_of };
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            let Ok(pid) = name_str.parse::<i32>() else {
                continue;
            };
            let stat_path = format!("/proc/{pid}/stat");
            let Ok(content) = std::fs::read_to_string(&stat_path) else {
                continue;
            };
            let ppid = content
                .rsplit(')')
                .next()
                .and_then(|s| s.split_whitespace().nth(1))
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            children_of.entry(ppid).or_default().push(pid);
        }
        Self { children_of }
    }

    /// Empty map on platforms without a `/proc` filesystem.
    #[cfg(not(target_os = "linux"))]
    pub fn snapshot() -> Self {
        Self {
            children_of: HashMap::new(),
        }
    }

    /// Every descendant of a pid, capped to avoid pathological loops.
    pub fn descendants_of(&self, pid: i32) -> Vec<i32> {
        let mut out = Vec::new();
        let mut stack = vec![pid];
        while let Some(p) = stack.pop() {
            if let Some(kids) = self.children_of.get(&p) {
                for &kid in kids {
                    if kid == p || kid == pid {
                        continue;
                    }
                    out.push(kid);
                    stack.push(kid);
                    if out.len() > 4096 {
                        return out;
                    }
                }
            }
        }
        out
    }
}

impl PaneStats {
    /// Sample one pid with a fresh map; prefer `sample_with_map` for many panes.
    pub fn sample(&mut self, pid: i32) {
        self.sample_with_map(pid, &ProcMap::snapshot());
    }

    /// Sample a pid and its descendants against an already built process map.
    pub fn sample_with_map(&mut self, pid: i32, map: &ProcMap) {
        let now = Instant::now();
        let children = map.descendants_of(pid);
        let mut ticks = read_cpu_ticks(pid);
        let mut rss_pages = read_rss_pages(pid);
        for cpid in &children {
            ticks += read_cpu_ticks(*cpid);
            rss_pages += read_rss_pages(*cpid);
        }

        if let Some(prev_time) = self.prev_time {
            let elapsed = now.duration_since(prev_time).as_secs_f64();
            if elapsed > 0.1 {
                let tick_delta = ticks.saturating_sub(self.prev_ticks) as f64;
                let clock_ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
                self.cpu_percent = ((tick_delta / clock_ticks_per_sec) / elapsed * 100.0) as f32;
            }
        }

        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as f32;
        self.mem_mb = rss_pages as f32 * page_size / (1024.0 * 1024.0);

        self.prev_ticks = ticks;
        self.prev_time = Some(now);
    }
}

#[cfg(target_os = "linux")]
fn read_cpu_ticks(pid: i32) -> u64 {
    let path = format!("/proc/{pid}/stat");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let fields: Vec<&str> = content
        .rsplit(')')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    if fields.len() < 13 {
        return 0;
    }
    let utime: u64 = fields[11].parse().unwrap_or(0);
    let stime: u64 = fields[12].parse().unwrap_or(0);
    utime + stime
}

#[cfg(not(target_os = "linux"))]
fn read_cpu_ticks(_pid: i32) -> u64 {
    0
}

#[cfg(target_os = "linux")]
fn read_rss_pages(pid: i32) -> u64 {
    let path = format!("/proc/{pid}/statm");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let fields: Vec<&str> = content.split_whitespace().collect();
    if fields.len() < 2 {
        return 0;
    }
    fields[1].parse().unwrap_or(0)
}

#[cfg(not(target_os = "linux"))]
fn read_rss_pages(_pid: i32) -> u64 {
    0
}

/// Optional memory and cpu ceilings for a pane, with a restart backoff.
#[derive(Clone, Default)]
pub struct ResourceLimits {
    pub mem_limit_mb: Option<f32>,
    pub cpu_limit_pct: Option<f32>,
    pub restart_delay_secs: u64,
}

impl ResourceLimits {
    /// Name the first ceiling these stats exceed, or `None` when within budget.
    pub fn check(&self, stats: &PaneStats) -> Option<&str> {
        if let Some(limit) = self.mem_limit_mb
            && stats.mem_mb > limit
            && stats.mem_mb > 0.0
        {
            return Some("mem_limit");
        }
        if let Some(limit) = self.cpu_limit_pct
            && stats.cpu_percent > limit
            && stats.cpu_percent > 0.0
        {
            return Some("cpu_limit");
        }
        None
    }
}

#[cfg(test)]
#[path = "tests/monitor.rs"]
mod tests;
