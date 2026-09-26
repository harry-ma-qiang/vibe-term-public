// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Foundation and presentation layers: pty spawning, terminal emulation, host
//! terminal setup, wire types, input, layout, painting, sampling and redaction.

/// Wire action and request types for decoded client requests.
pub mod capture;
/// Terminal emulator screen, cells, styles and scrollback.
pub mod emulator;
/// Secret redaction over an emulated screen.
pub mod filter;
/// Pane geometry, hit testing and border drawing.
pub mod frame;
/// Host key, mouse and paste input classification.
pub mod input;
/// Lock state machine for the multi-client input model.
pub mod lock;
/// Per-pane cpu and memory sampling, and resource limits.
pub mod monitor;
/// Pseudoterminal spawn, io, resize and reaping.
pub mod pty;
/// Full and diff painting of an emulated screen.
pub mod renderer;
/// Host terminal setup, restore and signal handling.
pub mod terminal;
/// Argument parsing for the `exec` and `serve` wire verbs.
pub mod wire_parser;
