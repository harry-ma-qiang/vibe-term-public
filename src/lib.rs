// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Foundation, presentation and session layers: pty spawning, terminal
//! emulation, layout, painting, redaction, recording and service panes.

/// Wire action and request types for decoded client requests.
pub mod capture;
/// Terminal emulator screen, cells, styles and scrollback.
pub mod emulator;
/// Secret redaction over an emulated screen.
pub mod filter;
/// Pane geometry, hit testing and border drawing.
pub mod frame;
/// Screen layout for the main pane, the sidebar stack and the status bar.
pub mod grid;
/// Host key, mouse and paste input classification.
pub mod input;
/// Lock state machine for the multi-client input model.
pub mod lock;
/// Per-pane cpu and memory sampling, and resource limits.
pub mod monitor;
/// Pseudoterminal spawn, io, resize and reaping.
pub mod pty;
/// Recording of an emulated screen to a cast file.
pub mod recorder;
/// Full and diff painting of an emulated screen.
pub mod renderer;
/// Service panes and the registry that builds them by name.
pub mod service;
/// The two-row status bar: text, painting and click mapping.
pub mod status_bar;
/// Host terminal setup, restore and signal handling.
pub mod terminal;
/// Argument parsing for the `exec` and `serve` wire verbs.
pub mod wire_parser;
