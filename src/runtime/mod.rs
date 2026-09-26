// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Cancelling a call in flight, and passing messages into a service pane.

/// Cancel tokens and the registry of in-flight streaming calls.
pub mod cancel;

pub use cancel::{CancelToken, StreamRegistry};
