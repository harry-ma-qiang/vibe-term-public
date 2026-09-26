// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for resolving a session name to its control socket.

use super::resolve_session;
use std::os::unix::net::UnixListener;

struct TestDir {
    path: std::path::PathBuf,
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn make_session_dir(name: &str) -> TestDir {
    let path = std::path::PathBuf::from(format!("/tmp/vibe-term-{name}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("mkdir test session dir");
    let sock = path.join("control.sock");
    let _ = std::fs::remove_file(&sock);
    let _listener = UnixListener::bind(&sock).expect("bind test socket");
    std::mem::forget(_listener);
    TestDir { path }
}

#[test]
fn resolve_session_finds_literal_name() {
    let name = format!("resolvetest-literal-{}", std::process::id());
    let _td = make_session_dir(&name);
    let resolved = resolve_session(&name);
    assert!(
        resolved.is_some(),
        "literal-name resolve should succeed for /tmp/vibe-term-{name}"
    );
    assert!(
        resolved.unwrap().to_string_lossy().contains(&name),
        "resolved path should contain the literal name"
    );
}

#[test]
fn resolve_session_falls_back_to_user_prefixed_form() {
    let user = std::env::var("USER").unwrap_or_else(|_| "uid-test".to_string());
    let bare = format!("resolvetestfallback{}", std::process::id());
    let prefixed = format!("{user}-{bare}");
    let _td = make_session_dir(&prefixed);
    let resolved = resolve_session(&bare);
    assert!(
        resolved.is_some(),
        "bare-name `{bare}` should fall back to `{prefixed}`"
    );
    assert!(
        resolved
            .unwrap()
            .to_string_lossy()
            .contains(&format!("vibe-term-{prefixed}")),
        "fallback should resolve to user-prefixed dir"
    );
}

#[test]
fn resolve_session_returns_none_when_neither_form_exists() {
    let bogus = format!("resolvetest-nope-{}", std::process::id());
    assert!(
        resolve_session(&bogus).is_none(),
        "non-existent session must return None"
    );
}

#[test]
fn resolve_session_does_not_prefix_hyphenated_names() {
    let user = std::env::var("USER").unwrap_or_else(|_| "uid-test".to_string());
    let inner = format!("resolvetesthyphen{}", std::process::id());
    let full = format!("{user}-{inner}");
    let double = format!("{user}-{full}");
    let _td = make_session_dir(&full);
    let resolved = resolve_session(&full);
    assert!(resolved.is_some(), "literal hyphenated name should resolve");
    assert!(
        !resolved.unwrap().to_string_lossy().contains(&double),
        "must not double-prefix a hyphenated name"
    );
}
