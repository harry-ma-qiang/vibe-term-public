// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Foundation, presentation and session layers: pty spawning, terminal
//! emulation, layout, painting, redaction, recording, panes and sessions.

/// The `signal` wire verb: its name safelist, parsing and delivery.
pub mod api_dispatch_signal;
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
/// One pane: a child pseudoterminal or a service, with its screen and log.
pub mod pane;
/// Pseudoterminal spawn, io, resize and reaping.
pub mod pty;
/// Recording of an emulated screen to a cast file.
pub mod recorder;
/// Full and diff painting of an emulated screen.
pub mod renderer;
/// Pane-local mouse selection, copied text and clipboard delivery.
pub mod selection;
/// Service panes and the registry that builds them by name.
pub mod service;
/// The session: panes, focus, layout inputs, recorder and session directory.
pub mod session;
/// The session side of the wire verbs: send, read, wait, close, acquire.
pub mod session_api;
/// Session lifecycle: session directory, resource limits and shutdown.
pub mod session_lifecycle;
/// Adding, removing and relaying out a session's panes.
pub mod session_pane;
/// The two-row status bar: text, painting and click mapping.
pub mod status_bar;
/// Host terminal setup, restore and signal handling.
pub mod terminal;
/// Argument parsing for the `exec` and `serve` wire verbs.
pub mod wire_parser;
