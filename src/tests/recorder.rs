// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for recording: the header line, keyframes, row diffs, monotonic and
//! idle-capped timestamps, markers and redacted content.

use super::*;
use crate::emulator::Screen;
use std::path::PathBuf;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("vibe-term-test-{name}.cast"))
}

#[test]
fn create_writes_header() {
    let p = temp_path("header");
    {
        let _rec = Recorder::create(&p, 80, 24).unwrap();
    }
    let content = std::fs::read_to_string(&p).unwrap();
    let header: serde_json::Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(header["version"], 2);
    assert_eq!(header["width"], 80);
    assert_eq!(header["height"], 24);
    std::fs::remove_file(&p).ok();
}

#[test]
fn record_frame_writes_event() {
    let p = temp_path("frame");
    let mut screen = Screen::new(40, 5);
    screen.feed(b"Hello recorder");

    let mut rec = Recorder::create(&p, 40, 5).unwrap();
    rec.record_frame(&screen).unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    let event: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert!(event[0].as_f64().unwrap() >= 0.0);
    assert_eq!(event[1], "o");
    assert!(event[2].as_str().unwrap().contains("Hello recorder"));
    std::fs::remove_file(&p).ok();
}

#[test]
fn monotonic_timestamps() {
    let p = temp_path("monotonic");
    let mut screen = Screen::new(20, 3);
    screen.feed(b"A");

    let mut rec = Recorder::create(&p, 20, 3).unwrap();
    rec.record_frame(&screen).unwrap();
    screen.feed(b"B");
    rec.record_frame(&screen).unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let events: Vec<serde_json::Value> = content
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    assert!(events.len() >= 2);
    let t0 = events[0][0].as_f64().unwrap();
    let t1 = events[1][0].as_f64().unwrap();
    assert!(t1 >= t0, "timestamps not monotonic: {t0} > {t1}");
    std::fs::remove_file(&p).ok();
}

#[test]
fn diff_skips_unchanged_rows() {
    let p = temp_path("diff");
    let mut screen = Screen::new(20, 5);
    screen.feed(b"static\r\nchanging");

    let mut rec = Recorder::create(&p, 20, 5).unwrap();
    rec.record_frame(&screen).unwrap();

    screen.clear_dirty();
    screen.feed(b"\x1b[2;1Hupdated!");
    rec.record_frame(&screen).unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let last_line = content.lines().last().unwrap();
    let event: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let data = event[2].as_str().unwrap();
    assert!(data.contains("updated!"), "diff missing change");
    assert!(!data.contains("static"), "diff contains unchanged row");
    std::fs::remove_file(&p).ok();
}

#[test]
fn filtered_content_in_recording() {
    use crate::filter::SecretFilter;
    let p = temp_path("filtered");
    let mut screen = Screen::new(40, 3);
    screen.feed(b"token=sk-live-abc123xyz end");

    let mut filter = SecretFilter::new();
    filter.add_literal("sk-live-abc123xyz");
    filter.scrub_screen(&mut screen);

    let mut rec = Recorder::create(&p, 40, 3).unwrap();
    rec.record_frame(&screen).unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    assert!(!content.contains("sk-live"), "secret in .cast file!");
    assert!(content.contains("\u{2588}"), "redaction missing from .cast");
    std::fs::remove_file(&p).ok();
}

#[test]
fn append_continues_timestamps() {
    let p = temp_path("append");
    let mut screen = Screen::new(20, 3);
    screen.feed(b"first");

    let mut rec = Recorder::create(&p, 20, 3).unwrap();
    rec.record_frame(&screen).unwrap();
    drop(rec);

    screen.feed(b"\r\nsecond");
    let mut rec2 = Recorder::append(&p, 20, 3).unwrap();
    rec2.record_frame(&screen).unwrap();
    drop(rec2);

    let content = std::fs::read_to_string(&p).unwrap();
    let events: Vec<serde_json::Value> = content
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    assert!(events.len() >= 2);
    let t0 = events[0][0].as_f64().unwrap();
    let t1 = events[1][0].as_f64().unwrap();
    assert!(t1 >= t0, "append timestamps not monotonic");
    std::fs::remove_file(&p).ok();
}

#[test]
fn frame_count_tracks() {
    let p = temp_path("count");
    let mut screen = Screen::new(20, 3);
    screen.feed(b"X");

    let mut rec = Recorder::create(&p, 20, 3).unwrap();
    assert_eq!(rec.frame_count(), 0);
    rec.record_frame(&screen).unwrap();
    assert_eq!(rec.frame_count(), 1);
    screen.feed(b"Y");
    rec.record_frame(&screen).unwrap();
    assert_eq!(rec.frame_count(), 2);
    drop(rec);
    std::fs::remove_file(&p).ok();
}

#[test]
fn diff_frame_smaller_than_keyframe() {
    let p = temp_path("diff-size");
    let mut screen = Screen::new(80, 24);
    screen.feed(b"Line 1 with lots of content here\r\n");
    screen.feed(b"Line 2 with more content\r\n");
    screen.feed(b"Line 3 even more text");

    let mut rec = Recorder::create(&p, 80, 24).unwrap();
    rec.record_frame(&screen).unwrap();

    screen.clear_dirty();
    screen.feed(b"\x1b[2;1Htiny");
    rec.record_frame(&screen).unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let events: Vec<&str> = content.lines().skip(1).collect();
    assert_eq!(events.len(), 2);

    let keyframe: serde_json::Value = serde_json::from_str(events[0]).unwrap();
    let diff: serde_json::Value = serde_json::from_str(events[1]).unwrap();
    let kf_data = keyframe[2].as_str().unwrap();
    let df_data = diff[2].as_str().unwrap();

    assert!(
        df_data.len() < kf_data.len(),
        "diff ({}) should be smaller than keyframe ({})",
        df_data.len(),
        kf_data.len()
    );
    assert!(df_data.contains("tiny"), "diff missing changed content");
    assert!(!df_data.contains("Line 1"), "diff contains unchanged row 1");
    assert!(!df_data.contains("Line 3"), "diff contains unchanged row 3");
    std::fs::remove_file(&p).ok();
}

#[test]
fn multiple_incremental_diffs() {
    let p = temp_path("multi-diff");
    let mut screen = Screen::new(40, 5);
    screen.feed(b"row0\r\nrow1\r\nrow2\r\nrow3\r\nrow4");

    let mut rec = Recorder::create(&p, 40, 5).unwrap();
    rec.record_frame(&screen).unwrap();

    for i in 0..3 {
        screen.clear_dirty();
        screen.feed(format!("\x1b[{};1Hchange{i}", i + 1).as_bytes());
        rec.record_frame(&screen).unwrap();
    }
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let events: Vec<serde_json::Value> = content
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    assert_eq!(events.len(), 4, "expected 1 keyframe + 3 diffs");

    for (offset, event) in events[1..4].iter().enumerate() {
        let data = event[2].as_str().unwrap();
        let expected = format!("change{offset}");
        assert!(
            data.contains(&expected),
            "diff {offset} missing '{expected}'"
        );
    }

    let data1 = events[1][2].as_str().unwrap();
    assert!(
        !data1.contains("row4"),
        "diff 1 should not contain untouched row4"
    );
    std::fs::remove_file(&p).ok();
}

#[test]
fn timestamps_monotonic_across_many_frames() {
    let p = temp_path("many-ts");
    let mut screen = Screen::new(20, 3);

    let mut rec = Recorder::create(&p, 20, 3).unwrap();
    for i in 0..10 {
        screen.feed(format!("{i}").as_bytes());
        rec.record_frame(&screen).unwrap();
    }
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let timestamps: Vec<f64> = content
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v[0].as_f64())
        .collect();

    assert_eq!(timestamps.len(), 10);
    for i in 1..timestamps.len() {
        assert!(
            timestamps[i] >= timestamps[i - 1],
            "timestamp[{i}]={} < timestamp[{}]={}",
            timestamps[i],
            i - 1,
            timestamps[i - 1]
        );
    }
    std::fs::remove_file(&p).ok();
}

#[test]
fn first_frame_is_keyframe_with_clear() {
    let p = temp_path("first-kf");
    let mut screen = Screen::new(20, 3);
    screen.feed(b"Hello");

    let mut rec = Recorder::create(&p, 20, 3).unwrap();
    rec.record_frame(&screen).unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let event: serde_json::Value = serde_json::from_str(content.lines().nth(1).unwrap()).unwrap();
    let data = event[2].as_str().unwrap();
    assert!(
        data.contains("\x1b[H\x1b[2J"),
        "first frame should clear screen (keyframe)"
    );
    assert!(data.contains("Hello"), "keyframe missing content");
    std::fs::remove_file(&p).ok();
}

#[test]
fn idle_compression_caps_gap() {
    let p = std::env::temp_dir().join("vibe_test_idle.cast");
    let mut screen = Screen::new(20, 3);
    screen.feed(b"frame1");

    let mut rec = Recorder::create(&p, 20, 3).unwrap();
    rec.set_idle_max(1.0);
    rec.record_frame(&screen).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(1500));

    screen.feed(b"\x1b[H");
    screen.feed(b"frame2");
    rec.record_frame(&screen).unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert!(lines.len() >= 3, "need header + 2 frames");
    let ts1: f64 = serde_json::from_str::<serde_json::Value>(lines[1]).unwrap()[0]
        .as_f64()
        .unwrap();
    let ts2: f64 = serde_json::from_str::<serde_json::Value>(lines[2]).unwrap()[0]
        .as_f64()
        .unwrap();
    let gap = ts2 - ts1;
    assert!(
        gap <= 1.1,
        "gap should be compressed to <= 1.0s, got {gap:.3}"
    );
    std::fs::remove_file(&p).ok();
}

#[test]
fn write_marker_adds_comment() {
    let p = std::env::temp_dir().join("vibe_test_marker.cast");
    let mut rec = Recorder::create(&p, 20, 3).unwrap();
    rec.write_marker("test-point").unwrap();
    drop(rec);

    let content = std::fs::read_to_string(&p).unwrap();
    assert!(
        content.contains("# marker: test-point"),
        "marker missing: {content}"
    );
    std::fs::remove_file(&p).ok();
}
