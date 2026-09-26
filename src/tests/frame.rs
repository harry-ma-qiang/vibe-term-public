// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for pane geometry: content rectangles, hit zones and border output.

use super::*;

#[test]
fn content_rect_subtracts_borders() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
        0,
        1234,
    );
    let c = f.content_rect();
    assert_eq!((c.x, c.y, c.w, c.h), (1, 1, 78, 22));
}

#[test]
fn content_rect_with_offset() {
    let f = Frame::new(
        Rect {
            x: 40,
            y: 10,
            w: 40,
            h: 12,
        },
        1,
        5678,
    );
    let c = f.content_rect();
    assert_eq!((c.x, c.y, c.w, c.h), (41, 11, 38, 10));
}

#[test]
fn child_size_is_content_dimensions() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        },
        0,
        1,
    );
    assert_eq!(f.child_size(), (98, 48));
}

#[test]
fn hit_test_outside() {
    let f = Frame::new(
        Rect {
            x: 10,
            y: 5,
            w: 40,
            h: 20,
        },
        0,
        1,
    );
    assert_eq!(f.hit_test(0, 0), HitZone::Outside);
    assert_eq!(f.hit_test(9, 5), HitZone::Outside);
    assert_eq!(f.hit_test(50, 5), HitZone::Outside);
    assert_eq!(f.hit_test(10, 25), HitZone::Outside);
}

#[test]
fn hit_test_title_bar() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
        0,
        1,
    );
    assert_eq!(f.hit_test(0, 0), HitZone::TitleBar);
    assert_eq!(f.hit_test(5, 0), HitZone::TitleBar);
    assert_eq!(f.hit_test(40, 0), HitZone::TitleBar);
}

#[test]
fn hit_test_close_button() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
        0,
        1,
    );
    assert_eq!(f.hit_test(78, 0), HitZone::CloseButton);
    assert_eq!(f.hit_test(77, 0), HitZone::CloseButton);
}

#[test]
fn hit_test_content() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
        0,
        1,
    );
    assert_eq!(f.hit_test(1, 1), HitZone::Content);
    assert_eq!(f.hit_test(40, 12), HitZone::Content);
    assert_eq!(f.hit_test(78, 22), HitZone::Content);
}

#[test]
fn hit_test_side_borders() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
        0,
        1,
    );
    assert_eq!(f.hit_test(0, 10), HitZone::Border);
    assert_eq!(f.hit_test(79, 10), HitZone::Border);
}

#[test]
fn hit_test_bottom_border() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
        0,
        1,
    );
    assert_eq!(f.hit_test(40, 23), HitZone::Border);
}

#[test]
fn render_border_contains_slot_and_pid() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 10,
        },
        3,
        9876,
    );
    let mut buf = String::new();
    f.render_border(&mut buf, false, false);
    assert!(buf.contains("#3"), "missing slot: {buf}");
    assert!(buf.contains("pid:9876"), "missing pid: {buf}");
    assert!(buf.contains("[x]"), "missing close button: {buf}");
    assert!(buf.contains("┌"), "missing top-left corner");
    assert!(buf.contains("┐"), "missing top-right corner");
    assert!(buf.contains("└"), "missing bottom-left corner");
    assert!(buf.contains("┘"), "missing bottom-right corner");
    assert!(buf.contains("│"), "missing side border");
}

#[test]
fn render_border_focused_uses_bright_color() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 10,
        },
        0,
        1,
    );
    let mut buf = String::new();
    f.render_border(&mut buf, true, false);
    assert!(buf.contains("\x1b[1;36m"), "focused should use bold cyan");
}

#[test]
fn render_border_unfocused_uses_dim_color() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 10,
        },
        0,
        1,
    );
    let mut buf = String::new();
    f.render_border(&mut buf, false, false);
    assert!(buf.contains("\x1b[90m"), "unfocused should use dim gray");
}

#[test]
fn tiny_frame_skips_render() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 3,
            h: 2,
        },
        0,
        1,
    );
    let mut buf = String::new();
    f.render_border(&mut buf, false, false);
    assert!(buf.is_empty(), "too small to render");
}

#[test]
fn content_rect_minimum_size() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
        },
        0,
        1,
    );
    let c = f.content_rect();
    assert_eq!(c.w, 0);
    assert_eq!(c.h, 0);
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut in_esc = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        if in_esc {
            if c.is_ascii_alphabetic() {
                in_esc = false;
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[test]
fn all_borders_same_display_width() {
    for w in [20, 40, 80, 104, 160] {
        let f = Frame::new(
            Rect {
                x: 0,
                y: 0,
                w,
                h: 5,
            },
            0,
            12345,
        );
        let mut buf = String::new();
        f.render_border(&mut buf, false, false);
        let stripped = strip_ansi(&buf);
        let top: String = stripped.chars().take(w).collect();
        assert_eq!(
            top.chars().count(),
            w,
            "top border width wrong for w={w}: got {}",
            top.chars().count()
        );
        assert!(top.starts_with('┌'), "top should start with ┌ for w={w}");
        assert!(top.ends_with('┐'), "top should end with ┐ for w={w}");
    }
}

#[test]
fn border_renders_copy_button_left_of_close() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 5,
        },
        1,
        99,
    );
    let mut buf = String::new();
    f.render_border(&mut buf, false, false);
    assert!(buf.contains("[c][x]"), "expected [c][x] run, got: {buf:?}");
}

#[test]
fn hit_test_copy_button() {
    let f = Frame::new(
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 5,
        },
        1,
        99,
    );
    assert_eq!(f.hit_test(37, 0), HitZone::CloseButton);
    assert_eq!(f.hit_test(34, 0), HitZone::CopyButton);
    assert_eq!(f.hit_test(10, 0), HitZone::TitleBar);
}

#[test]
fn copy_and_close_zones_do_not_overlap() {
    let f = Frame::new(
        Rect {
            x: 5,
            y: 2,
            w: 30,
            h: 6,
        },
        1,
        99,
    );
    let mut copy = 0;
    let mut close = 0;
    for x in 5..35 {
        match f.hit_test(x, 2) {
            HitZone::CopyButton => copy += 1,
            HitZone::CloseButton => close += 1,
            _ => {}
        }
    }
    assert_eq!(copy, 3, "copy zone must be exactly 3 cells");
    assert_eq!(close, 3, "close zone must be exactly 3 cells");
}

#[test]
fn hit_zones_match_rendered_button_columns() {
    for w in [24usize, 30, 40, 60, 80] {
        let f = Frame::new(
            Rect {
                x: 3,
                y: 1,
                w,
                h: 6,
            },
            1,
            42,
        );
        let mut buf = String::new();
        f.render_border(&mut buf, false, false);
        let line = buf.lines().next().unwrap_or("");
        let plain: String = strip_ansi(line);
        let chars: Vec<char> = plain.chars().collect();
        let find = |pat: [char; 3]| {
            (0..chars.len().saturating_sub(2))
                .find(|&i| chars[i] == pat[0] && chars[i + 1] == pat[1] && chars[i + 2] == pat[2])
                .expect("button rendered")
        };
        let cpos = find(['[', 'c', ']']);
        let xpos = find(['[', 'x', ']']);
        for i in 0..3 {
            assert_eq!(
                f.hit_test(3 + cpos + i, 1),
                HitZone::CopyButton,
                "w={w} copy col {}",
                cpos + i
            );
            assert_eq!(
                f.hit_test(3 + xpos + i, 1),
                HitZone::CloseButton,
                "w={w} close col {}",
                xpos + i
            );
        }
    }
}
