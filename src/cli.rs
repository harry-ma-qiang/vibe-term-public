// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The options a session is started with, parsed from an argument list.

use clap::Parser;
use std::path::PathBuf;

/// Session start options: name, pane commands, layout, redaction and limits.
#[derive(Parser)]
#[command(name = "vibe-term", about = "Start a vibe-term session")]
pub struct Cli {
    #[arg(
        long,
        value_name = "FILE",
        help = "Load secret literals from a KEY=VALUE file"
    )]
    pub filter: Option<PathBuf>,
    #[arg(long, help = "Disable alternate screen (for testing)")]
    pub no_alt_screen: bool,
    #[arg(long, help = "Run as background daemon (detached)")]
    pub daemon: bool,
    #[arg(long, value_name = "CMD", help = "Command for sidebar slot 1")]
    pub slot1: Option<String>,
    #[arg(long, value_name = "CMD", help = "Command for sidebar slot 2")]
    pub slot2: Option<String>,
    #[arg(long, value_name = "CMD", help = "Command for sidebar slot 3")]
    pub slot3: Option<String>,
    #[arg(long, value_name = "CMD", help = "Command for sidebar slot 4")]
    pub slot4: Option<String>,
    #[arg(long, value_name = "FILE", help = "Load blocklist from file")]
    pub filter_blocklist: Option<PathBuf>,
    #[arg(long, default_value = "left", help = "Sidebar position: left or right")]
    pub side: String,
    #[arg(long, value_name = "NAME", help = "Session name")]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "MB",
        help = "Memory limit per pane (MB, kills if exceeded)"
    )]
    pub mem_limit: Option<f32>,
    #[arg(
        long,
        value_name = "PCT",
        help = "CPU limit per pane (%, kills if exceeded)"
    )]
    pub cpu_limit: Option<f32>,
    #[arg(
        long,
        default_value = "3",
        help = "Seconds to wait before restart after limit kill"
    )]
    pub restart_delay: u64,
    #[arg(long, help = "Delete session files on exit (default: keep)")]
    pub clean_session: bool,
    #[arg(last = true, help = "Command to run in main pane")]
    pub command: Vec<String>,
}

#[cfg(test)]
#[path = "tests/cli.rs"]
mod tests;
