// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for secret redaction: dotenv parsing, literals, regex shapes and
//! the user-supplied word list.

use super::*;

#[test]
fn parse_dotenv_basic() {
    let content = "KEY=value123\nSECRET=s3cretpass\n";
    let vals = parse_dotenv(content);
    assert_eq!(vals, vec!["value123", "s3cretpass"]);
}

#[test]
fn parse_dotenv_quoted() {
    let content = "A=\"quoted_value\"\nB='single_quoted'\n";
    let vals = parse_dotenv(content);
    assert_eq!(vals, vec!["quoted_value", "single_quoted"]);
}

#[test]
fn parse_dotenv_export_prefix() {
    let content = "export TOKEN=abc123xyz\n";
    let vals = parse_dotenv(content);
    assert_eq!(vals, vec!["abc123xyz"]);
}

#[test]
fn parse_dotenv_comments_and_blanks() {
    let content = "# comment\n\nKEY=value1\n  # another\nKEY2=value2\n";
    let vals = parse_dotenv(content);
    assert_eq!(vals, vec!["value1", "value2"]);
}

#[test]
fn parse_dotenv_value_with_equals() {
    let content = "URL=postgres://user:pass@db:5432/app\n";
    let vals = parse_dotenv(content);
    assert_eq!(vals, vec!["postgres://user:pass@db:5432/app"]);
}

#[test]
fn load_dotenv_from_file() {
    let dir = std::env::temp_dir().join("vibe-term-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.env");
    std::fs::write(&path, "API_KEY=sk-live-test123\nSHORT=abc\n").unwrap();

    let mut f = SecretFilter::new();
    f.load_dotenv(&path).unwrap();
    assert_eq!(f.literal_count(), 1);

    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn load_dotenv_missing_file_warns() {
    let mut f = SecretFilter::new();
    let result = f.load_dotenv(Path::new("/nonexistent/file.env"));
    assert!(result.is_ok());
    assert_eq!(f.literal_count(), 0);
}

#[test]
fn filter_skips_short_secrets() {
    let mut f = SecretFilter::new();
    f.add_literal("abc");
    assert_eq!(f.literal_count(), 0);
    assert!(!f.is_enabled());
    f.add_literal("abcdef");
    assert_eq!(f.literal_count(), 1);
    assert!(f.is_enabled());
}

#[test]
fn filter_disabled_by_default() {
    let f = SecretFilter::new();
    assert!(!f.is_enabled());
}

#[test]
fn filter_auto_enables_on_add() {
    let mut f = SecretFilter::new();
    assert!(!f.is_enabled());
    f.add_literal("long_secret_value");
    assert!(f.is_enabled());
}

#[test]
fn enable_disable_toggle() {
    let mut f = SecretFilter::new();
    assert!(!f.is_enabled());
    f.enable();
    assert!(f.is_enabled());
    f.disable();
    assert!(!f.is_enabled());
}

#[test]
fn disabled_filter_does_not_scrub() {
    let mut screen = Screen::new(40, 3);
    screen.feed(b"password is s3cretpass here");

    let mut f = SecretFilter::new();
    f.add_literal("s3cretpass");
    assert!(f.is_enabled());

    f.disable();
    f.scrub_screen(&mut screen);
    assert_eq!(screen.row_text(0), "password is s3cretpass here");
}

#[test]
fn scrub_screen_redacts_secret() {
    let mut screen = Screen::new(40, 3);
    screen.feed(b"password is s3cretpass here");

    let mut filter = SecretFilter::new();
    filter.add_literal("s3cretpass");
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(!text.contains("s3cretpass"), "secret leaked: {text}");
    assert!(text.contains("██████████"), "missing redaction: {text}");
    assert!(text.contains("password is"), "context lost: {text}");
    assert!(text.contains("here"), "context lost: {text}");
}

#[test]
fn scrub_preserves_layout_width() {
    let mut screen = Screen::new(40, 3);
    screen.feed(b"key=s3cretpass end");

    let before_chars: usize = screen.row_text(0).chars().count();

    let mut filter = SecretFilter::new();
    filter.add_literal("s3cretpass");
    filter.scrub_screen(&mut screen);

    let after = screen.row_text(0);
    let after_chars: usize = after.chars().count();
    assert_eq!(
        after_chars, before_chars,
        "layout char count changed: {after}"
    );
}

#[test]
fn scrub_multiline_spanning() {
    let mut screen = Screen::new(13, 3);
    screen.feed(b"...secret_val");
    screen.feed(b"ue!remains...");

    let mut filter = SecretFilter::new();
    filter.add_literal("secret_value!");

    filter.scrub_screen(&mut screen);

    let r0 = screen.row_text(0);
    let r1 = screen.row_text(1);
    assert!(!r0.contains("secret"), "row0 leaked: {r0}");
    assert!(!r1.contains("ue!"), "row1 leaked: {r1}");
    assert!(r1.contains("remains"), "context lost: {r1}");
}

#[test]
fn scrub_newline_does_not_span() {
    let mut screen = Screen::new(20, 3);
    screen.feed(b"API_KEY=secret_val\r\nue_next_line");

    let mut filter = SecretFilter::new();
    filter.add_literal("secret_value_next_line");

    filter.scrub_screen(&mut screen);

    let r0 = screen.row_text(0);
    let r1 = screen.row_text(1);
    assert!(
        r0.contains("secret_val"),
        "explicit newline should break match: {r0}"
    );
    assert!(
        r1.contains("ue_next_line"),
        "explicit newline should break match: {r1}"
    );
}

#[test]
fn scrub_no_false_positive() {
    let mut screen = Screen::new(40, 3);
    screen.feed(b"keyboard shortcut here");

    let mut filter = SecretFilter::new();
    filter.add_literal("key");
    filter.scrub_screen(&mut screen);

    assert_eq!(screen.row_text(0), "keyboard shortcut here");
}

#[test]
fn longest_match_first() {
    let mut screen = Screen::new(40, 3);
    screen.feed(b"token=sk-live-abc123xyz-end");

    let mut filter = SecretFilter::new();
    filter.add_literal("sk-live");
    filter.add_literal("sk-live-abc123xyz");
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(!text.contains("abc123"), "partial match leaked: {text}");
}

#[test]
fn regex_catches_api_keys() {
    let mut screen = Screen::new(80, 3);
    screen.feed(b"key: sk-ant-api03-xyzABCdef123456789abcdef");

    let mut filter = SecretFilter::new();
    filter.load_bundled_patterns();
    filter.enable();
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(
        !text.contains("xyzABCdef"),
        "regex should catch API key: {text}"
    );
}

#[test]
fn regex_catches_github_token() {
    let mut screen = Screen::new(80, 3);
    screen.feed(b"token: ghp_abcdefghijklmnopqrstuvwxyz1234567890");

    let mut filter = SecretFilter::new();
    filter.load_bundled_patterns();
    filter.enable();
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(
        !text.contains("ghp_abcdef"),
        "regex should catch GitHub token: {text}"
    );
}

#[test]
fn regex_catches_aws_key() {
    let mut screen = Screen::new(80, 3);
    screen.feed(b"AWS_KEY=AKIAIOSFODNN7EXAMPLE");

    let mut filter = SecretFilter::new();
    filter.load_bundled_patterns();
    filter.enable();
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(
        !text.contains("AKIAIOSFODNN7"),
        "regex should catch AWS key: {text}"
    );
}

#[test]
fn blocklist_matches_entries() {
    let dir = std::env::temp_dir();
    let path = dir.join("vibe_test_blocklist.txt");
    std::fs::write(&path, "my-internal-hostname\nmy-secret-email@corp.com\n").unwrap();

    let mut screen = Screen::new(80, 3);
    screen.feed(b"connecting to my-internal-hostname on port 443");

    let mut filter = SecretFilter::new();
    filter.load_blocklist(&path).unwrap();
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(
        !text.contains("my-internal-hostname"),
        "blocklist should catch: {text}"
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn three_layers_combined() {
    let mut screen = Screen::new(80, 3);
    screen.feed(b"lit=MYSECRETVALUE key=ghp_abcdefghijklmnopqrstuvwxyz1234567890");

    let mut filter = SecretFilter::new();
    filter.add_literal("MYSECRETVALUE");
    filter.load_bundled_patterns();
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(!text.contains("MYSECRETVALUE"), "literal layer: {text}");
    assert!(!text.contains("ghp_abcdef"), "regex layer: {text}");
}

#[test]
fn regex_performance_under_1ms() {
    let mut filter = SecretFilter::new();
    filter.load_bundled_patterns();
    filter.enable();

    let mut screen = Screen::new(80, 24);
    for r in 0..24 {
        let line = format!(
            "\x1b[{};1HLine {} with some text and key=AKIAIOSFODNN7EXAMPLE end",
            r + 1,
            r
        );
        screen.feed(line.as_bytes());
    }

    let start = std::time::Instant::now();
    for _ in 0..100 {
        filter.scrub_screen(&mut screen);
    }
    let elapsed = start.elapsed();
    let per_frame_us = elapsed.as_micros() / 100;
    assert!(
        per_frame_us < 2000,
        "regex scrub should be <2ms, got {}us",
        per_frame_us
    );
}

#[test]
fn bundled_pattern_count() {
    let mut filter = SecretFilter::new();
    filter.load_bundled_patterns();
    assert!(
        filter.regex_count() >= 15,
        "need >= 15 patterns, got {}",
        filter.regex_count()
    );
}

#[test]
fn user_literal_file_is_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("filter-literals.txt");
    std::fs::write(&path, "# a comment\nacme-internal-host\nshort\n").unwrap();

    let mut screen = Screen::new(80, 3);
    screen.feed(b"connecting to acme-internal-host on port 443");

    let mut filter = SecretFilter::new();
    filter.load_optional_blocklist(&path).unwrap();
    assert_eq!(
        filter.blocklist_count(),
        1,
        "comment and short line skipped"
    );
    assert!(filter.is_enabled());
    filter.scrub_screen(&mut screen);

    let text = screen.row_text(0);
    assert!(
        !text.contains("acme-internal-host"),
        "supplied literal should be redacted: {text}"
    );
    assert!(text.contains("port 443"), "context lost: {text}");
}

#[test]
fn missing_user_literal_file_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent-literals.txt");

    let mut filter = SecretFilter::new();
    let result = filter.load_optional_blocklist(&path);
    assert!(result.is_ok(), "a missing word list must not be an error");
    assert_eq!(filter.blocklist_count(), 0);
    assert!(!filter.is_enabled());
}

#[test]
fn literals_path_defaults_under_config_dir() {
    if std::env::var(ENV_FILTER_LITERALS).is_ok() {
        return;
    }
    let path = literals_path();
    assert!(
        path.ends_with("vibe-term/filter-literals.txt"),
        "unexpected default: {path:?}"
    );
    assert!(path.is_absolute() || std::env::var("HOME").is_err());
}

#[test]
fn no_literals_are_built_in() {
    let mut filter = SecretFilter::new();
    filter.load_bundled_patterns();
    assert_eq!(filter.literal_count(), 0);
    assert_eq!(filter.blocklist_count(), 0);
}
