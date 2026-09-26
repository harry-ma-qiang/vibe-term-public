// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the pid file round trip and the cwd fallback chain, without
//! forking or touching the runner's stdio.

use super::*;
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;

#[test]
fn pid_file_round_trip_mode_0600() {
    let dir = tempdir().expect("tempdir");
    let h = DaemonHandle {
        pid: 4242,
        socket: PathBuf::from("/tmp/x.sock"),
        start_time_unix_secs: 1_777_939_200,
    };
    write_pid_file(dir.path(), &h).expect("write");
    let (pid, sock, when) = read_pid_file(dir.path()).expect("read");
    assert_eq!(pid, 4242);
    assert_eq!(sock, PathBuf::from("/tmp/x.sock"));
    assert_eq!(when, "2026-05-05T00:00:00Z");
    let meta = std::fs::metadata(dir.path().join(DAEMON_PID_FILENAME)).expect("meta");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn chdir_with_fallback_target_dir_wins() {
    let dir = tempdir().expect("tempdir");
    let saved = std::env::current_dir().ok();
    let landed = chdir_with_fallback(Some(dir.path()));
    let landed_canon = landed.canonicalize().unwrap_or(landed);
    let target_canon = dir
        .path()
        .canonicalize()
        .unwrap_or(dir.path().to_path_buf());
    assert_eq!(landed_canon, target_canon);
    if let Some(s) = saved {
        let _ = std::env::set_current_dir(s);
    }
}

#[test]
fn chdir_with_fallback_none_falls_to_home_or_root() {
    let saved = std::env::current_dir().ok();
    let landed = chdir_with_fallback(None);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let ok = landed.as_path() == Path::new("/")
        || home
            .as_deref()
            .map(|h| h == landed.as_path())
            .unwrap_or(false);
    assert!(
        ok,
        "expected landed in $HOME or /, got {landed:?} (HOME={home:?})"
    );
    if let Some(s) = saved {
        let _ = std::env::set_current_dir(s);
    }
}

#[test]
fn chdir_with_fallback_invalid_target_falls_through() {
    let saved = std::env::current_dir().ok();
    let bogus = PathBuf::from("/nonexistent-fallback-test-path");
    let landed = chdir_with_fallback(Some(&bogus));
    assert_ne!(landed, bogus, "must fall through when target chdir fails");
    if let Some(s) = saved {
        let _ = std::env::set_current_dir(s);
    }
}
