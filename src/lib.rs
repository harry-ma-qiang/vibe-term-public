// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Foundation layer: pty spawning, terminal emulation, host terminal setup,
//! wire argument parsing and the input lock state machine.

/// Terminal emulator screen, cells, styles and scrollback.
pub mod emulator;
/// Lock state machine for the multi-client input model.
pub mod lock;
/// Pseudoterminal spawn, io, resize and reaping.
pub mod pty;
/// Host terminal setup, restore and signal handling.
pub mod terminal;
/// Argument parsing for the `exec` and `serve` wire verbs.
pub mod wire_parser;
