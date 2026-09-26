// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Named keys the `send` verb accepts, and the bytes each one sends.

/// The byte sequence for a named key, or `None` when the name is unknown.
pub fn parse_key_name(name: &str) -> Option<Vec<u8>> {
    Some(match name {
        "enter" | "return" => vec![b'\r'],
        "tab" => vec![b'\t'],
        "escape" | "esc" => vec![0x1b],
        "backspace" => vec![0x7f],
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "pageup" => b"\x1b[5~".to_vec(),
        "pagedown" => b"\x1b[6~".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        "ctrl-c" => vec![0x03],
        "ctrl-d" => vec![0x04],
        "ctrl-z" => vec![0x1a],
        "ctrl-l" => vec![0x0c],
        "ctrl-a" => vec![0x01],
        "ctrl-e" => vec![0x05],
        "ctrl-k" => vec![0x0b],
        "ctrl-u" => vec![0x15],
        "ctrl-w" => vec![0x17],
        "ctrl-r" => vec![0x12],
        _ => return None,
    })
}
