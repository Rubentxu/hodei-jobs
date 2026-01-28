//!
//! # Worker Events Upcasting
//!
//! Schema evolution for worker domain events. Provides upcasters to handle
//! event schema changes between versions.
//!
//! ## Event Schema Versioning
//!
//! - Version 1: Initial event schema (original events)
//! - Version 2: Current schema with enhanced metadata and new fields

use serde_json::Value;

use saga_engine_core::event::upcasting::{
    EventTypeId, EventUpcaster, EventUpcasterRegistry, UpcastError,
};

// ============================================================================
// Event Type Identifiers
// ============================================================================

/// Event type identifiers for worker events
pub mod worker_event_types {
    use saga_engine_core::event::upcasting::EventTypeId;

    pub const WORKER_REGISTERED: EventTypeId = EventTypeId("worker.registered");
    pub const WORKER_STATUS_CHANGED: EventTypeId = EventTypeId("worker.status_changed");
    pub const WORKER_TERMINATED: EventTypeId = EventTypeId("worker.terminated");
    pub const WORKER_DISCONNECTED: EventTypeId = EventTypeId("worker.disconnected");
    pub const WORKER_PROVISIONED: EventTypeId = EventTypeId("worker.provisioned");
    pub const WORKER_RECONNECTED: EventTypeId = EventTypeId("worker.reconnected");
    pub const WORKER_RECOVERY_FAILED: EventTypeId = EventTypeId("worker.recovery_failed");
    pub const WORKER_READY_FOR_JOB: EventTypeId = EventTypeId("worker.ready_for_job");
    pub const WORKER_PROVISIONING_REQUESTED: EventTypeId =
        EventTypeId("worker.provisioning_requested");
    pub const WORKER_HEARTBEAT: EventTypeId = EventTypeId("worker.heartbeat");
    pub const WORKER_READY: EventTypeId = EventTypeId("worker.ready");
    pub const WORKER_STATE_UPDATED: EventTypeId = EventTypeId("worker.state_updated");
    pub const WORKER_SELF_TERMINATED: EventTypeId = EventTypeId("worker.self_terminated");
}

// ============================================================================
// Schema Version Constants
// ============================================================================

/// Event schema version for worker events
pub const WORKER_EVENT_VERSION: u32 = 2;

// ============================================================================
// Upcasters for Worker Events
// ============================================================================

/// Upcaster for WorkerRegistered events (v1 -> v2)
pub struct WorkerRegisteredUpcaster;

impl WorkerRegisteredUpcaster {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkerRegisteredUpcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcaster for WorkerRegisteredUpcaster {
    fn from_version(&self) -> u32 {
        1
    }

    fn to_version(&self) -> u32 {
        WORKER_EVENT_VERSION
    }

    fn event_type(&self) -> EventTypeId {
        worker_event_types::WORKER_REGISTERED
    }

    fn upcast(
        &self,
        _event_type: EventTypeId,
        _version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut result = payload.clone();
        result["_event_version"] = serde_json::json!(WORKER_EVENT_VERSION);

        if !result.is_object() || !result.as_object().unwrap().contains_key("correlation_id") {
            result["correlation_id"] = serde_json::json!(None::<String>);
        }

        if !result.is_object() || !result.as_object().unwrap().contains_key("actor") {
            result["actor"] = serde_json::json!(None::<String>);
        }

        Ok(result)
    }
}

/// Upcaster for WorkerStatusChanged events (v1 -> v2)
pub struct WorkerStatusChangedUpcaster;

impl WorkerStatusChangedUpcaster {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkerStatusChangedUpcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcaster for WorkerStatusChangedUpcaster {
    fn from_version(&self) -> u32 {
        1
    }

    fn to_version(&self) -> u32 {
        WORKER_EVENT_VERSION
    }

    fn event_type(&self) -> EventTypeId {
        worker_event_types::WORKER_STATUS_CHANGED
    }

    fn upcast(
        &self,
        _event_type: EventTypeId,
        _version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut result = payload.clone();
        result["_event_version"] = serde_json::json!(WORKER_EVENT_VERSION);

        if !result.is_object() || !result.as_object().unwrap().contains_key("correlation_id") {
            result["correlation_id"] = serde_json::json!(None::<String>);
        }

        if !result.is_object() || !result.as_object().unwrap().contains_key("actor") {
            result["actor"] = serde_json::json!(None::<String>);
        }

        Ok(result)
    }
}

/// Upcaster for WorkerTerminated events (v1 -> v2)
pub struct WorkerTerminatedUpcaster;

impl WorkerTerminatedUpcaster {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkerTerminatedUpcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcaster for WorkerTerminatedUpcaster {
    fn from_version(&self) -> u32 {
        1
    }

    fn to_version(&self) -> u32 {
        WORKER_EVENT_VERSION
    }

    fn event_type(&self) -> EventTypeId {
        worker_event_types::WORKER_TERMINATED
    }

    fn upcast(
        &self,
        _event_type: EventTypeId,
        _version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut result = payload.clone();
        result["_event_version"] = serde_json::json!(WORKER_EVENT_VERSION);

        if !result.is_object() || !result.as_object().unwrap().contains_key("correlation_id") {
            result["correlation_id"] = serde_json::json!(None::<String>);
        }

        if !result.is_object() || !result.as_object().unwrap().contains_key("actor") {
            result["actor"] = serde_json::json!(None::<String>);
        }

        Ok(result)
    }
}

/// Identity upcaster for events already at current version
pub struct WorkerIdentityUpcaster {
    event_type: EventTypeId,
}

impl WorkerIdentityUpcaster {
    #[inline]
    pub fn new(event_type: &'static str) -> Self {
        Self {
            event_type: EventTypeId(event_type),
        }
    }
}

impl EventUpcaster for WorkerIdentityUpcaster {
    fn from_version(&self) -> u32 {
        WORKER_EVENT_VERSION - 1
    }

    fn to_version(&self) -> u32 {
        WORKER_EVENT_VERSION
    }

    fn event_type(&self) -> EventTypeId {
        self.event_type
    }

    fn upcast(
        &self,
        _event_type: EventTypeId,
        _version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut result = payload.clone();
        result["_event_version"] = serde_json::json!(WORKER_EVENT_VERSION);
        Ok(result)
    }
}

// ============================================================================
// Upcaster Registry for Worker Events
// ============================================================================

/// Registry containing all upcasters for worker domain events
pub struct WorkerEventUpcasterRegistry {
    registry: EventUpcasterRegistry,
}

impl WorkerEventUpcasterRegistry {
    #[inline]
    pub fn new() -> Self {
        Self {
            registry: EventUpcasterRegistry::new(),
        }
    }
}

impl Default for WorkerEventUpcasterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerEventUpcasterRegistry {
    /// Register all worker event upcasters
    pub fn register_all(&mut self) {
        // Version 1 -> 2 upcasters for events that may have old schema
        self.registry.register(WorkerRegisteredUpcaster::new());
        self.registry.register(WorkerStatusChangedUpcaster::new());
        self.registry.register(WorkerTerminatedUpcaster::new());

        // Identity upcasters for events that were already at v2 or later
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.disconnected"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.provisioned"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.reconnected"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.recovery_failed"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.ready_for_job"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.provisioning_requested"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.heartbeat"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.ready"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.state_updated"));
        self.registry
            .register(WorkerIdentityUpcaster::new("worker.self_terminated"));
    }

    #[inline]
    pub fn registry(&self) -> &EventUpcasterRegistry {
        &self.registry
    }

    #[inline]
    pub fn registry_mut(&mut self) -> &mut EventUpcasterRegistry {
        &mut self.registry
    }

    #[inline]
    pub fn upcast_payload(
        &self,
        event_type: EventTypeId,
        current_version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        self.registry
            .upcast_payload(event_type, current_version, payload)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.registry.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }
}

pub use self::worker_event_types::*;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::event::upcasting::EventTypeId;

    #[test]
    fn test_worker_registered_upcaster() {
        let upcaster = WorkerRegisteredUpcaster::new();
        assert_eq!(upcaster.from_version(), 1);
        assert_eq!(upcaster.to_version(), WORKER_EVENT_VERSION);
        assert_eq!(upcaster.event_type(), worker_event_types::WORKER_REGISTERED);

        let payload = serde_json::json!({
            "worker_id": "worker-123",
            "provider_id": "provider-1",
            "occurred_at": "2024-01-01T00:00:00Z"
        });

        let result = upcaster
            .upcast(worker_event_types::WORKER_REGISTERED, 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], WORKER_EVENT_VERSION);
        assert_eq!(result["worker_id"], "worker-123");
    }

    #[test]
    fn test_worker_identity_upcaster() {
        let upcaster = WorkerIdentityUpcaster::new("worker.heartbeat");
        assert_eq!(upcaster.from_version(), 1);
        assert_eq!(upcaster.to_version(), WORKER_EVENT_VERSION);

        let payload = serde_json::json!({
            "worker_id": "worker-456",
            "state": "idle"
        });

        let result = upcaster
            .upcast(EventTypeId("worker.heartbeat"), 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], WORKER_EVENT_VERSION);
    }

    #[test]
    fn test_worker_event_registry() {
        let mut registry = WorkerEventUpcasterRegistry::new();
        registry.register_all();

        assert!(!registry.is_empty());
        assert!(
            registry
                .registry()
                .has_upcaster(worker_event_types::WORKER_REGISTERED, 1)
        );
        assert!(
            registry
                .registry()
                .has_upcaster(worker_event_types::WORKER_STATUS_CHANGED, 1)
        );
    }

    #[test]
    fn test_upcast_worker_registered_event() {
        let mut registry = WorkerEventUpcasterRegistry::new();
        registry.register_all();

        let payload = serde_json::json!({
            "worker_id": "worker-789",
            "provider_id": "provider-2",
            "occurred_at": "2024-01-01T00:00:00Z"
        });

        let result = registry
            .upcast_payload(worker_event_types::WORKER_REGISTERED, 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], WORKER_EVENT_VERSION);
        assert_eq!(result["worker_id"], "worker-789");
    }
}
