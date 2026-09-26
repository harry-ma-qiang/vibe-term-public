// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Foundation, presentation and session layers: pty spawning, terminal
//! emulation, layout, painting, redaction, recording, panes and sessions.

/// The `signal` wire verb: its name safelist, parsing and delivery.
pub mod api_dispatch_signal;
/// The request parser for the public verbs, and decoded request types.
pub mod capture;
/// Running exec and serve requests and answering them when done.
pub mod commands;
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
/// One pane: a child pseudoterminal or a service, with its screen.
pub mod pane;
/// Diff and full painting of one pane, with redaction and selection overlays.
pub mod pane_render;
/// Pseudoterminal spawn, io, resize and reaping.
pub mod pty;
/// Recording of an emulated screen to a cast file.
pub mod recorder;
/// Full and diff painting of an emulated screen.
pub mod renderer;
/// Verb runtime: callsites, permissions and the result envelope.
pub mod runtime;
/// Pane-local mouse selection, copied text and clipboard delivery.
pub mod selection;
/// The control socket: viewers, one-shot requests and pending waits.
pub mod server;
/// Service panes and the registry that builds them by name.
pub mod service;
/// The session: panes, focus, layout, recorder and finished panes' output.
pub mod session;
/// The session side of the wire verbs: send, read, wait, close, acquire.
pub mod session_api;
/// Session lifecycle: session directory, resource limits and shutdown.
pub mod session_lifecycle;
/// Adding, removing and relaying out a session's panes.
pub mod session_pane;
/// Painting a whole session, and the composited screen the recorder writes.
pub mod session_render;
/// The quit-confirm toast shown on the status bar's second row.
pub mod session_toolbar;
/// The two-row status bar: text, painting and click mapping.
pub mod status_bar;
/// Host terminal setup, restore and signal handling.
pub mod terminal;
/// Argument parsing for the `exec` and `serve` wire verbs.
pub mod wire_parser;
