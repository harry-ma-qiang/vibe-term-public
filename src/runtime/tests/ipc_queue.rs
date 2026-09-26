// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the service inbox: provenance tags, fifo order, cloned senders
//! and the default constructor.

use super::{IpcMessage, IpcProvenance, IpcQueue};

#[test]
fn new_queue_is_empty() {
    let q = IpcQueue::new();
    assert!(q.try_recv().is_err());
}

#[test]
fn from_tui_sends_with_correct_provenance() {
    let q = IpcQueue::new();
    let sender = q.sender();
    sender.from_tui("test tui".to_string());
    let msg = q.try_recv().expect("message should be received");
    assert_eq!(msg.text, "test tui");
    assert!(matches!(msg.provenance, IpcProvenance::Tui));
}

#[test]
fn from_verb_sends_with_correct_provenance() {
    let q = IpcQueue::new();
    let sender = q.sender();
    sender.from_verb("test verb".to_string());
    let msg = q.try_recv().expect("message should be received");
    assert_eq!(msg.text, "test verb");
    assert!(matches!(msg.provenance, IpcProvenance::Verb));
}

#[test]
fn from_mp_sends_with_correct_provenance() {
    let q = IpcQueue::new();
    let sender = q.sender();
    sender.from_mp("test mp".to_string());
    let msg = q.try_recv().expect("message should be received");
    assert_eq!(msg.text, "test mp");
    assert!(matches!(msg.provenance, IpcProvenance::Mp));
}

#[test]
fn messages_delivered_in_fifo_order() {
    let q = IpcQueue::new();
    let sender = q.sender();
    sender.from_tui("first".to_string());
    sender.from_verb("second".to_string());
    sender.from_mp("third".to_string());
    let msg1 = q.try_recv().expect("first message");
    assert_eq!(msg1.text, "first");
    let msg2 = q.try_recv().expect("second message");
    assert_eq!(msg2.text, "second");
    let msg3 = q.try_recv().expect("third message");
    assert_eq!(msg3.text, "third");
    assert!(q.try_recv().is_err());
}

#[test]
fn multiple_sends_before_recv_maintain_order() {
    let q = IpcQueue::new();
    let sender = q.sender();
    for i in 0..5 {
        sender.from_tui(format!("msg-{i}"));
    }
    for i in 0..5 {
        let msg = q.try_recv().expect("message");
        assert_eq!(msg.text, format!("msg-{i}"));
    }
    assert!(q.try_recv().is_err());
}

#[test]
fn sender_clone_sends_to_same_queue() {
    let q = IpcQueue::new();
    let s1 = q.sender();
    let s2 = s1.clone();
    let s3 = q.sender();
    s1.from_tui("a".to_string());
    s2.from_verb("b".to_string());
    s3.from_mp("c".to_string());
    let mut texts: Vec<String> = Vec::new();
    while let Ok(msg) = q.try_recv() {
        texts.push(msg.text);
    }
    assert_eq!(texts, vec!["a", "b", "c"]);
}

#[test]
fn default_queue_works() {
    let q = IpcQueue::default();
    let sender = q.sender();
    sender.from_tui("default".to_string());
    let msg = q.try_recv().expect("message");
    assert_eq!(msg.text, "default");
}

#[test]
fn ipc_message_clone_preserves_fields() {
    let msg = IpcMessage {
        text: "hello".to_string(),
        provenance: IpcProvenance::Tui,
    };
    let cloned = msg.clone();
    assert_eq!(cloned.text, "hello");
    assert!(matches!(cloned.provenance, IpcProvenance::Tui));
}

#[test]
fn ipc_provenance_debug_is_readable() {
    let tui = format!("{:?}", IpcProvenance::Tui);
    let verb = format!("{:?}", IpcProvenance::Verb);
    let mp = format!("{:?}", IpcProvenance::Mp);
    assert!(tui.contains("Tui"));
    assert!(verb.contains("Verb"));
    assert!(mp.contains("Mp"));
}
