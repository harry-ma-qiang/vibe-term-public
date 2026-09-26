// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the per-user session name and the command line's name handling.

use super::{check_name, is_error_reply, take_name};
use std::sync::Mutex;
use vibe_term::daemon::default_session_name;

static USER_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_user() -> std::sync::MutexGuard<'static, ()> {
    USER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn default_session_name_uses_env_user() {
    let _g = lock_user();
    let saved = std::env::var("USER").ok();
    unsafe {
        std::env::set_var("USER", "test_user_default");
    }
    let name = default_session_name("default");
    assert_eq!(name, "test_user_default-default");
    match saved {
        Some(v) => unsafe { std::env::set_var("USER", v) },
        None => unsafe { std::env::remove_var("USER") },
    }
}

#[test]
fn default_session_name_includes_session_key() {
    let _g = lock_user();
    let saved = std::env::var("USER").ok();
    unsafe {
        std::env::set_var("USER", "alice");
    }
    let work = default_session_name("work");
    let scratch = default_session_name("scratch");
    assert_eq!(work, "alice-work");
    assert_eq!(scratch, "alice-scratch");
    match saved {
        Some(v) => unsafe { std::env::set_var("USER", v) },
        None => unsafe { std::env::remove_var("USER") },
    }
}

#[test]
fn default_session_name_falls_back_to_uid_when_user_unset() {
    let _g = lock_user();
    let saved = std::env::var("USER").ok();
    unsafe {
        std::env::remove_var("USER");
    }
    let name = default_session_name("default");
    assert!(
        name.ends_with("-default"),
        "name should end with -default: {name}"
    );
    let user_part = name.trim_end_matches("-default");
    assert!(
        !user_part.is_empty() && user_part != "-",
        "user portion must be non-empty: {name}"
    );
    match saved {
        Some(v) => unsafe { std::env::set_var("USER", v) },
        None => unsafe { std::env::remove_var("USER") },
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn name_is_taken_only_from_the_front_of_the_arguments() {
    let args = strs(&["--name", "work", "echo", "--name"]);
    let (name, rest) = take_name(&args).unwrap();
    assert_eq!(name.as_deref(), Some("work"));
    assert_eq!(rest, &strs(&["echo", "--name"])[..]);
    let args = strs(&["--name=work", "1"]);
    let (name, rest) = take_name(&args).unwrap();
    assert_eq!(name.as_deref(), Some("work"));
    assert_eq!(rest, &strs(&["1"])[..]);
    let args = strs(&["echo", "hi"]);
    let (name, rest) = take_name(&args).unwrap();
    assert!(name.is_none());
    assert_eq!(rest.len(), 2);
}

#[test]
fn a_name_that_could_escape_the_session_directory_is_refused() {
    assert!(check_name("work-1_a.b").is_ok());
    for bad in ["", "..", ".hidden", "a/b", "x/../../etc", "a b"] {
        assert!(check_name(bad).is_err(), "should refuse {bad:?}");
    }
    assert!(take_name(&strs(&["--name"])).is_err());
}

#[test]
fn only_an_error_field_marks_a_reply_as_failed() {
    assert!(is_error_reply("{\"error\":\"pane not found: 9\"}\n"));
    assert!(!is_error_reply("{\"exit_code\":3,\"output\":\"x\"}\n"));
    assert!(!is_error_reply("not json\n"));
}
