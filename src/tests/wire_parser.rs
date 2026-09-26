// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for `exec` and `serve` argv parsing.

use super::*;

#[test]
fn simple_command() {
    let result = parse_exec_args(&["echo", "hello"]).unwrap();
    assert_eq!(result.child_cmd, "echo hello");
    assert_eq!(result.timeout_secs, 300);
    assert!(!result.no_timeout);
}

#[test]
fn command_with_timeout() {
    let result =
        parse_exec_args(&["--vt-timeout", "10", "cargo", "test", "--timeout", "30"]).unwrap();
    assert_eq!(result.child_cmd, "cargo test --timeout 30");
    assert_eq!(result.timeout_secs, 10);
    assert!(!result.no_timeout);
}

#[test]
fn double_dash_separator() {
    let result = parse_exec_args(&["--", "--version"]).unwrap();
    assert_eq!(result.child_cmd, "--version");
    assert_eq!(result.timeout_secs, 300);
}

#[test]
fn double_dash_with_timeout() {
    let result = parse_exec_args(&["--vt-timeout", "5", "--", "ls", "--help"]).unwrap();
    assert_eq!(result.child_cmd, "ls --help");
    assert_eq!(result.timeout_secs, 5);
}

#[test]
fn no_command_error() {
    let result = parse_exec_args(&[]);
    assert_eq!(result, Err(ParseError::NoCommand));
}

#[test]
fn only_timeout_no_command() {
    let result = parse_exec_args(&["--vt-timeout", "10"]);
    assert_eq!(result, Err(ParseError::NoCommand));
}

#[test]
fn timeout_zero_means_no_timeout() {
    let result = parse_exec_args(&["--vt-timeout", "0", "sleep", "5"]).unwrap();
    assert_eq!(result.child_cmd, "sleep 5");
    assert_eq!(result.timeout_secs, 0);
    assert!(result.no_timeout);
}

#[test]
fn negative_timeout_error() {
    let result = parse_exec_args(&["--vt-timeout", "-5", "echo", "hi"]);
    assert!(matches!(result, Err(ParseError::InvalidTimeout(_))));
}

#[test]
fn non_numeric_timeout_error() {
    let result = parse_exec_args(&["--vt-timeout", "abc", "echo", "hi"]);
    assert!(matches!(result, Err(ParseError::InvalidTimeout(_))));
}

#[test]
fn timeout_missing_value() {
    let result = parse_exec_args(&["--vt-timeout"]);
    assert!(matches!(result, Err(ParseError::InvalidTimeout(_))));
}

#[test]
fn first_unrecognized_token_starts_child() {
    let result = parse_exec_args(&["cargo", "test", "--release"]).unwrap();
    assert_eq!(result.child_cmd, "cargo test --release");
}

#[test]
fn single_quoted_multiline() {
    let result = parse_exec_args(&["for i in 1 2 3; do echo $i; done"]).unwrap();
    assert_eq!(result.child_cmd, "for i in 1 2 3; do echo $i; done");
}

#[test]
fn double_dash_only_no_command() {
    let result = parse_exec_args(&["--"]);
    assert_eq!(result, Err(ParseError::NoCommand));
}

#[test]
fn default_timeout_value() {
    let result = parse_exec_args(&["echo", "test"]).unwrap();
    assert_eq!(result.timeout_secs, 300);
}

#[test]
fn serve_with_pattern() {
    let result = parse_serve_args(&["npm", "run", "dev", "listening"]).unwrap();
    assert_eq!(result.child_cmd, "npm run dev");
    assert_eq!(result.pattern, Some("listening".to_string()));
    assert_eq!(result.timeout_secs, 300);
    assert!(!result.no_timeout);
}

#[test]
fn serve_with_timeout_and_pattern() {
    let result =
        parse_serve_args(&["--vt-timeout", "30", "npm", "run", "dev", "listening"]).unwrap();
    assert_eq!(result.child_cmd, "npm run dev");
    assert_eq!(result.pattern, Some("listening".to_string()));
    assert_eq!(result.timeout_secs, 30);
}

#[test]
fn serve_no_pattern_single_token() {
    let result = parse_serve_args(&["npm"]).unwrap();
    assert_eq!(result.child_cmd, "npm");
    assert_eq!(result.pattern, None);
}

#[test]
fn serve_no_pattern_with_timeout() {
    let result = parse_serve_args(&["--vt-timeout", "10", "npm"]).unwrap();
    assert_eq!(result.child_cmd, "npm");
    assert_eq!(result.pattern, None);
    assert_eq!(result.timeout_secs, 10);
}

#[test]
fn serve_no_command_error() {
    let result = parse_serve_args(&[]);
    assert_eq!(result, Err(ParseError::NoCommand));
}

#[test]
fn serve_only_timeout_no_command() {
    let result = parse_serve_args(&["--vt-timeout", "10"]);
    assert_eq!(result, Err(ParseError::NoCommand));
}

#[test]
fn serve_double_dash_with_pattern() {
    let result = parse_serve_args(&[
        "--vt-timeout",
        "5",
        "--",
        "python3",
        "-m",
        "http.server",
        "Serving",
    ])
    .unwrap();
    assert_eq!(result.child_cmd, "python3 -m http.server");
    assert_eq!(result.pattern, Some("Serving".to_string()));
    assert_eq!(result.timeout_secs, 5);
}

#[test]
fn serve_two_tokens_last_is_pattern() {
    let result = parse_serve_args(&["sleep", "ready"]).unwrap();
    assert_eq!(result.child_cmd, "sleep");
    assert_eq!(result.pattern, Some("ready".to_string()));
}

#[test]
fn serve_timeout_zero_no_timeout() {
    let result = parse_serve_args(&["--vt-timeout", "0", "npm", "start", "ready"]).unwrap();
    assert_eq!(result.child_cmd, "npm start");
    assert_eq!(result.pattern, Some("ready".to_string()));
    assert!(result.no_timeout);
}

#[test]
fn serve_invalid_timeout() {
    let result = parse_serve_args(&["--vt-timeout", "abc", "npm", "start"]);
    assert!(matches!(result, Err(ParseError::InvalidTimeout(_))));
}

#[test]
fn serve_quoted_command_with_pattern() {
    let result = parse_serve_args(&["echo not-the-pattern && exit 1", "ready"]).unwrap();
    assert_eq!(result.child_cmd, "echo not-the-pattern && exit 1");
    assert_eq!(result.pattern, Some("ready".to_string()));
}
