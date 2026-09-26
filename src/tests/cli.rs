// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for parsing session start options.

use super::Cli;
use clap::Parser;

#[test]
fn dashdash_cmd_preserves_command_path() {
    let cli = Cli::parse_from(["vibe-term", "--", "bash"]);
    assert!(cli.name.is_none());
    assert_eq!(cli.command, vec!["bash".to_string()]);
}

#[test]
fn name_and_command_parse_together() {
    let cli = Cli::parse_from([
        "vibe-term",
        "--daemon",
        "--name",
        "work",
        "--",
        "sh",
        "-c",
        "x",
    ]);
    assert!(cli.daemon);
    assert_eq!(cli.name.as_deref(), Some("work"));
    assert_eq!(cli.command, vec!["sh", "-c", "x"]);
}
