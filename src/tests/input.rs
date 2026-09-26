// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for key-to-byte mapping and bracketed-paste batching.

use super::*;

#[test]
fn ctrl_backslash_forwards() {
    let key = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL);
    match classify_key(key) {
        InputAction::Forward(bytes) => assert_eq!(bytes, vec![0x1c]),
        _ => panic!("expected Forward"),
    }
}

#[test]
fn regular_char_forwards() {
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    match classify_key(key) {
        InputAction::Forward(bytes) => assert_eq!(bytes, b"a"),
        _ => panic!("expected Forward"),
    }
}

#[test]
fn enter_forwards_cr() {
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    match classify_key(key) {
        InputAction::Forward(bytes) => assert_eq!(bytes, b"\r"),
        _ => panic!("expected Forward"),
    }
}

#[test]
fn arrow_keys_forward_escapes() {
    let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    match classify_key(key) {
        InputAction::Forward(bytes) => assert_eq!(bytes, b"\x1b[A"),
        _ => panic!("expected Forward"),
    }
}

#[test]
fn ctrl_c_forwards_as_control() {
    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    match classify_key(key) {
        InputAction::Forward(bytes) => assert_eq!(bytes, vec![3]),
        _ => panic!("expected Forward"),
    }
}

#[test]
fn ctrl_d_forwards_as_eof() {
    let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
    match classify_key(key) {
        InputAction::Forward(bytes) => assert_eq!(bytes, vec![4]),
        _ => panic!("expected Forward"),
    }
}

#[test]
fn pageup_forwards() {
    let key = KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
    match classify_key(key) {
        InputAction::Forward(bytes) => assert_eq!(bytes, b"\x1b[5~"),
        _ => panic!("expected Forward"),
    }
}

#[test]
fn bracketed_paste_batches_into_single_forward() {
    // why: a kilobyte paste must become one forward, byte for byte identical.
    let body: String = (0..1024).map(|i| (b'A' + (i as u8 % 26)) as char).collect();
    let action = classify_paste(body.clone());
    match action {
        InputAction::Forward(bytes) => {
            assert_eq!(bytes.len(), 1024);
            assert_eq!(bytes, body.as_bytes());
        }
        _ => panic!("expected Forward, got non-batched action"),
    }
}

#[test]
fn empty_paste_is_noop() {
    let action = classify_paste(String::new());
    assert!(
        matches!(action, InputAction::None),
        "empty paste must be None, not Forward([])"
    );
}

#[test]
fn paste_with_multiline_preserves_bytes() {
    let body = "line1\nline2\nline3\n".to_string();
    let action = classify_paste(body.clone());
    match action {
        InputAction::Forward(bytes) => assert_eq!(bytes, body.as_bytes()),
        _ => panic!("expected Forward"),
    }
}
