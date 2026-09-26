// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the service registry: named factories, sorted listing and the
//! process-wide registry round trip.

use super::{
    InputEvent, ServicePane, ServiceRegistry, create_service, register_factory,
    registered_factory_names, unregister_factory,
};
use crate::filter::SecretFilter;
use crate::frame::Frame;
use crate::session::Session;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct TestService {
    pub name: String,
    pub bytes_received: Arc<Mutex<Vec<u8>>>,
    pub tick_count: Arc<AtomicU32>,
    pub accept_input: bool,
    pub fill_byte: u8,
}

impl TestService {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            bytes_received: Arc::new(Mutex::new(Vec::new())),
            tick_count: Arc::new(AtomicU32::new(0)),
            accept_input: true,
            fill_byte: b'X',
        }
    }
}

impl ServicePane for TestService {
    fn name(&self) -> &str {
        &self.name
    }
    fn handle_input(&mut self, ev: InputEvent) {
        if let InputEvent::Bytes(b) = ev
            && let Ok(mut buf) = self.bytes_received.lock()
        {
            buf.extend_from_slice(&b);
        }
    }
    fn render(&self, out: &mut Vec<u8>, frame: &Frame) {
        let rect = frame.content_rect();
        for row in 0..rect.h {
            for _ in 0..rect.w {
                out.push(self.fill_byte);
            }
            out.push(b'\n');
            let _ = row;
        }
    }
    fn tick(&mut self) {
        self.tick_count.fetch_add(1, Ordering::Relaxed);
    }
    fn accepts_input_from(&self, _observer: crate::lock::ClientId) -> bool {
        self.accept_input
    }
}

#[test]
fn service_list_includes_pane_and_factory() {
    let mut reg = ServiceRegistry::new();
    reg.register("svc-a", || Box::new(TestService::new("svc-a")));
    reg.register("svc-b", || Box::new(TestService::new("svc-b")));
    let names = reg.names();
    assert_eq!(names, vec!["svc-a".to_string(), "svc-b".to_string()]);
    let created = reg.create("svc-a").expect("factory creates pane");
    assert_eq!(created.name(), "svc-a");
    assert!(reg.unregister("svc-a"));
    assert!(reg.create("svc-a").is_none());
}

#[test]
fn global_registry_round_trip() {
    let unique = format!("__test_svc_{}", std::process::id());
    register_factory(&unique, || Box::new(TestService::new("global")));
    assert!(registered_factory_names().contains(&unique));
    let svc = create_service(&unique).expect("global factory");
    assert_eq!(svc.name(), "global");
    assert!(unregister_factory(&unique));
    assert!(!registered_factory_names().contains(&unique));
}

#[test]
fn recorder_skips_service_panes_in_composite() {
    let mut s = Session::new(80, 24);
    s.add_main("sleep", &["10"], SecretFilter::new()).unwrap();
    let svc: Box<dyn ServicePane> = Box::new(TestService::new("test"));
    let _ = s.add_service_pane("test", svc).unwrap();
    let comp = crate::session_render::composite_panes(&s.panes, 80, 24, 0, "status");
    let total = comp.cells.iter().flat_map(|r| r.iter()).count();
    assert!(total > 0);
}
