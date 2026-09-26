// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Host input: reads key, mouse and paste events, and maps them to the bytes
//! or pointer actions the pane layer acts on.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// What one host input event asks for: bytes to forward, or a pointer action.
pub enum InputAction {
    Forward(Vec<u8>),
    MouseClick { x: u16, y: u16 },
    MouseDrag { x: u16, y: u16 },
    MouseUp { x: u16, y: u16 },
    MouseMove { x: u16, y: u16 },
    ScrollUp { x: u16, y: u16 },
    ScrollDown { x: u16, y: u16 },
    None,
}

/// Wait up to `timeout` for one event, returning `None` if none arrives.
pub fn poll_input(timeout: Duration) -> InputAction {
    if !event::poll(timeout).unwrap_or(false) {
        return InputAction::None;
    }
    read_input()
}

/// Block until one host event arrives and classify it.
pub fn read_input() -> InputAction {
    match event::read() {
        Ok(Event::Key(key)) => classify_key(key),
        Ok(Event::Mouse(mouse)) => classify_mouse(mouse),
        // why: a bracketed paste arrives as one event; forward it in one pty write.
        Ok(Event::Paste(s)) => classify_paste(s),
        _ => InputAction::None,
    }
}

/// Batch a bracketed paste into one forward. Pure, so it is unit-testable.
pub fn classify_paste(s: String) -> InputAction {
    if s.is_empty() {
        InputAction::None
    } else {
        InputAction::Forward(s.into_bytes())
    }
}

fn classify_key(key: KeyEvent) -> InputAction {
    let bytes = key_to_bytes(key);
    if bytes.is_empty() {
        InputAction::None
    } else {
        InputAction::Forward(bytes)
    }
}

fn classify_mouse(mouse: crossterm::event::MouseEvent) -> InputAction {
    use crossterm::event::MouseEventKind;
    match mouse.kind {
        MouseEventKind::ScrollUp => InputAction::ScrollUp {
            x: mouse.column,
            y: mouse.row,
        },
        MouseEventKind::ScrollDown => InputAction::ScrollDown {
            x: mouse.column,
            y: mouse.row,
        },
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => InputAction::MouseClick {
            x: mouse.column,
            y: mouse.row,
        },
        MouseEventKind::Drag(crossterm::event::MouseButton::Left) => InputAction::MouseDrag {
            x: mouse.column,
            y: mouse.row,
        },
        MouseEventKind::Up(crossterm::event::MouseButton::Left) => InputAction::MouseUp {
            x: mouse.column,
            y: mouse.row,
        },
        MouseEventKind::Moved => InputAction::MouseMove {
            x: mouse.column,
            y: mouse.row,
        },
        _ => InputAction::None,
    }
}

fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let ctrl = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    '^' => 0x1e,
                    '_' => 0x1f,
                    '@' => 0x00,
                    '[' => 0x1b,
                    _ => return vec![],
                };
                vec![ctrl]
            } else if key.modifiers.contains(KeyModifiers::ALT) {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                let mut bytes = vec![0x1b];
                bytes.extend_from_slice(s.as_bytes());
                bytes
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                s.as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => b"\x1b[1;3A".to_vec(),
        KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => b"\x1b[1;3B".to_vec(),
        KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => b"\x1b[1;3C".to_vec(),
        KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => b"\x1b[1;3D".to_vec(),
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

/// Decode an SGR mouse report from raw bytes into a pointer action.
pub fn parse_raw_input(data: &[u8]) -> Option<InputAction> {
    let s = std::str::from_utf8(data).ok()?;
    if let Some(inner) = s.strip_prefix("\x1b[<") {
        let press = inner.ends_with('M');
        let release = inner.ends_with('m');
        if !press && !release {
            return None;
        }
        let coords = &inner[..inner.len() - 1];
        let parts: Vec<&str> = coords.split(';').collect();
        if parts.len() != 3 {
            return None;
        }
        let btn: u16 = parts[0].parse().ok()?;
        let x: u16 = parts[1].parse::<u16>().ok()?.saturating_sub(1);
        let y: u16 = parts[2].parse::<u16>().ok()?.saturating_sub(1);
        if btn == 0 && press {
            return Some(InputAction::MouseClick { x, y });
        }
        if btn == 64 {
            return Some(InputAction::ScrollUp { x, y });
        }
        if btn == 65 {
            return Some(InputAction::ScrollDown { x, y });
        }
        if btn == 32 && press {
            return Some(InputAction::MouseDrag { x, y });
        }
        if btn == 0 && release {
            return Some(InputAction::MouseUp { x, y });
        }
        if btn == 35 {
            return Some(InputAction::MouseMove { x, y });
        }
    }
    None
}

#[cfg(test)]
#[path = "tests/input.rs"]
mod tests;
