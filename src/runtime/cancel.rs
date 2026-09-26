// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Cooperative cancellation for streaming verbs: a shared flag and the
//! registry that hands out one flag per in-flight stream id.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A shared flag a streaming worker polls to learn it should stop.
#[derive(Clone, Debug)]
pub struct CancelToken {
    pub flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// Create an uncancelled token.
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// Request cancellation; idempotent.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Monotonic stream ids mapped to the cancel token of each live stream.
#[derive(Clone, Debug, Default)]
pub struct StreamRegistry {
    inner: Arc<StreamRegistryInner>,
}

#[derive(Debug, Default)]
struct StreamRegistryInner {
    next_id: AtomicU64,
    active: Mutex<HashMap<u64, CancelToken>>,
}

impl StreamRegistry {
    /// Create an empty registry whose first allocated id is one.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(StreamRegistryInner {
                next_id: AtomicU64::new(1),
                active: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Register a new stream, returning its id and cancel token.
    pub fn allocate(&self) -> (u64, CancelToken) {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let token = CancelToken::new();
        if let Ok(mut active) = self.inner.active.lock() {
            active.insert(id, token.clone());
        }
        (id, token)
    }

    /// Forget a stream id; later cancels for it do nothing.
    pub fn release(&self, id: u64) {
        if let Ok(mut active) = self.inner.active.lock() {
            active.remove(&id);
        }
    }

    /// Cancel a live stream, reporting whether the id was known.
    pub fn cancel(&self, id: u64) -> bool {
        if let Ok(active) = self.inner.active.lock()
            && let Some(token) = active.get(&id)
        {
            token.cancel();
            return true;
        }
        false
    }

    /// Returns a closure that releases the given stream id when invoked.
    pub fn release_fn(&self, id: u64) -> impl FnOnce() + Send + 'static {
        let cloned = self.clone();
        move || cloned.release(id)
    }
}

#[cfg(test)]
#[path = "tests/cancel.rs"]
mod tests;
