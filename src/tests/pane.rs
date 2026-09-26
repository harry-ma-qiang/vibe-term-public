// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the pane status command shown in a border title: the shell
//! wrapper is stripped, every other argv is shown as given.

use super::display_cmd;

#[test]
fn shell_wrapper_shows_payload_only() {
    assert_eq!(display_cmd("bash", &["-c", "cargo test"]), "cargo test");
    assert_eq!(display_cmd("zsh", &["-c", "ls -la"]), "ls -la");
    assert_eq!(
        display_cmd("bash", &["-c", "bash -c 'echo hi'"]),
        "bash -c 'echo hi'"
    );
}

#[test]
fn non_wrapper_cmds_unchanged() {
    assert_eq!(display_cmd("vim", &["file.txt"]), "vim file.txt");
    assert_eq!(display_cmd("bash", &[]), "bash");
    assert_eq!(display_cmd("bash", &["-lc", "x"]), "bash -lc x");
}
