// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Service panes: panes backed by a supervised process rather than a
//! pseudoterminal, plus the registry that builds them by name.

use crate::frame::Frame;
use crate::lock::ClientId;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Exit-status probing and the health badge it produces.
pub mod health;
/// Restart policy, crash counting and backoff for a supervised child.
pub mod supervisor;
/// A service pane driven by a user-supplied shell command.
pub mod user;

pub use health::{HealthCheck, HealthStatus};
pub use supervisor::{CrashCounter, RestartPolicy, ServiceState};
pub use user::{UserService, UserServiceSpec};

/// Input delivered to a service pane by the host.
pub enum InputEvent {
    Bytes(Vec<u8>),
    Click { x: u16, y: u16 },
    Resize { cols: u16, rows: u16 },
}

/// A pane whose content is produced by a service rather than a child terminal.
pub trait ServicePane: Send {
    /// Name this pane is registered and addressed under.
    fn name(&self) -> &str;
    /// Consume one input event from the host.
    fn handle_input(&mut self, ev: InputEvent);
    /// Paint the pane into the buffer, inside the frame's content rectangle.
    fn render(&self, out: &mut Vec<u8>, frame: &Frame);
    /// Advance any time-based state; called once per host tick.
    fn tick(&mut self) {}
    /// Stop background work before the pane leaves the grid.
    fn shutdown(&mut self) {}
    /// Whether input from this observer may reach the service.
    fn accepts_input_from(&self, _observer_id: ClientId) -> bool {
        true
    }
    /// Machine-readable pane state for the wire protocol.
    fn status_json(&self) -> Option<serde_json::Value> {
        None
    }
    /// Title-bar badge; the empty string draws none.
    fn health_badge(&self) -> &'static str {
        ""
    }
    /// Whether the service has reached its terminal crashed state.
    fn is_crashed(&self) -> bool {
        false
    }
    /// Clear the crash count and respawn; a no-op without a restart policy.
    fn restart_self(&mut self) {}
}

/// Boxed constructor for one named kind of service pane.
pub type ServiceFactory = Box<dyn Fn() -> Box<dyn ServicePane> + Send + Sync>;

/// Factories for service panes, keyed by name.
pub struct ServiceRegistry {
    factories: HashMap<String, ServiceFactory>,
}

impl ServiceRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a factory, replacing any factory of the same name.
    pub fn register<F>(&mut self, name: &str, factory: F)
    where
        F: Fn() -> Box<dyn ServicePane> + Send + Sync + 'static,
    {
        self.factories.insert(name.to_string(), Box::new(factory));
    }

    /// Remove a factory; true when one was registered under the name.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.factories.remove(name).is_some()
    }

    /// Build a service pane from the named factory.
    pub fn create(&self, name: &str) -> Option<Box<dyn ServicePane>> {
        self.factories.get(name).map(|f| f())
    }

    /// Registered names, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.factories.keys().cloned().collect();
        out.sort();
        out
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_REGISTRY: OnceLock<Mutex<ServiceRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<ServiceRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| Mutex::new(ServiceRegistry::new()))
}

/// Register a factory in the process-wide registry.
pub fn register_factory<F>(name: &str, factory: F)
where
    F: Fn() -> Box<dyn ServicePane> + Send + Sync + 'static,
{
    if let Ok(mut reg) = registry().lock() {
        reg.register(name, factory);
    }
}

/// Remove a factory from the process-wide registry.
pub fn unregister_factory(name: &str) -> bool {
    registry()
        .lock()
        .map(|mut reg| reg.unregister(name))
        .unwrap_or(false)
}

/// Build a service pane from the process-wide registry.
pub fn create_service(name: &str) -> Option<Box<dyn ServicePane>> {
    registry().lock().ok().and_then(|reg| reg.create(name))
}

/// Names held by the process-wide registry, sorted.
pub fn registered_factory_names() -> Vec<String> {
    registry().lock().map(|reg| reg.names()).unwrap_or_default()
}

#[cfg(test)]
#[path = "../tests/service.rs"]
mod tests;
