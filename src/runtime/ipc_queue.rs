// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! An in-process message queue into a service pane, tagged by who sent it.

use std::sync::mpsc;

/// Which side of the program a queued message came from.
#[derive(Clone, Debug)]
pub enum IpcProvenance {
    Tui,
    Verb,
    Mp,
}

/// One queued message and where it came from.
#[derive(Clone, Debug)]
pub struct IpcMessage {
    pub text: String,
    pub provenance: IpcProvenance,
}

/// The sending half of an `IpcQueue`; cheap to clone.
pub struct IpcSender {
    tx: mpsc::Sender<IpcMessage>,
}

impl IpcSender {
    fn send(&self, text: String, provenance: IpcProvenance) {
        let _ = self.tx.send(IpcMessage { text, provenance });
    }

    /// Queue a message from the interactive user.
    pub fn from_tui(&self, text: String) {
        self.send(text, IpcProvenance::Tui);
    }

    /// Queue a message from a wire verb.
    pub fn from_verb(&self, text: String) {
        self.send(text, IpcProvenance::Verb);
    }

    /// Queue a message from a multi-party peer.
    pub fn from_mp(&self, text: String) {
        self.send(text, IpcProvenance::Mp);
    }
}

impl Clone for IpcSender {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

/// A service pane's inbox: the receiver, plus a sender to hand out.
pub struct IpcQueue {
    rx: mpsc::Receiver<IpcMessage>,
    pub sender: IpcSender,
}

impl IpcQueue {
    /// An empty queue.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            rx,
            sender: IpcSender { tx },
        }
    }

    /// The next queued message, without blocking.
    pub fn try_recv(&self) -> Result<IpcMessage, mpsc::TryRecvError> {
        self.rx.try_recv()
    }

    /// A new sender into this queue.
    pub fn sender(&self) -> IpcSender {
        self.sender.clone()
    }
}

impl Default for IpcQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/ipc_queue.rs"]
mod tests;
