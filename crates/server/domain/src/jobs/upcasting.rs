//!
//! # Job Events Upcasting
//!
//! Schema evolution for job domain events. Provides upcasters to handle
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

/// Event type identifiers for job events
pub mod job_event_types {
    use saga_engine_core::event::upcasting::EventTypeId;

    pub const JOB_CREATED: EventTypeId = EventTypeId("job.created");
    pub const JOB_STATUS_CHANGED: EventTypeId = EventTypeId("job.status_changed");
    pub const JOB_CANCELLED: EventTypeId = EventTypeId("job.cancelled");
    pub const JOB_RETRYED: EventTypeId = EventTypeId("job.retried");
    pub const JOB_ASSIGNED: EventTypeId = EventTypeId("job.assigned");
    pub const JOB_ACCEPTED: EventTypeId = EventTypeId("job.accepted");
    pub const JOB_DISPATCH_ACKNOWLEDGED: EventTypeId = EventTypeId("job.dispatch_acknowledged");
    pub const RUN_JOB_RECEIVED: EventTypeId = EventTypeId("job.run_job_received");
    pub const JOB_QUEUED: EventTypeId = EventTypeId("job.queued");
    pub const JOB_EXECUTION_ERROR: EventTypeId = EventTypeId("job.execution_error");
    pub const JOB_DISPATCH_FAILED: EventTypeId = EventTypeId("job.dispatch_failed");
}

// ============================================================================
// Schema Version Constants
// ============================================================================

/// Event schema version for job events
pub const JOB_EVENT_VERSION: u32 = 2;

// ============================================================================
// Upcasters for Job Events
// ============================================================================

/// Upcaster for JobCreated events (v1 -> v2)
pub struct JobCreatedUpcaster;

impl JobCreatedUpcaster {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for JobCreatedUpcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcaster for JobCreatedUpcaster {
    fn from_version(&self) -> u32 {
        1
    }

    fn to_version(&self) -> u32 {
        JOB_EVENT_VERSION
    }

    fn event_type(&self) -> EventTypeId {
        job_event_types::JOB_CREATED
    }

    fn upcast(
        &self,
        _event_type: EventTypeId,
        _version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut result = payload.clone();
        result["_event_version"] = serde_json::json!(JOB_EVENT_VERSION);

        if !result.is_object() || !result.as_object().unwrap().contains_key("correlation_id") {
            result["correlation_id"] = serde_json::json!(None::<String>);
        }

        if !result.is_object() || !result.as_object().unwrap().contains_key("actor") {
            result["actor"] = serde_json::json!(None::<String>);
        }

        Ok(result)
    }
}

/// Upcaster for JobStatusChanged events (v1 -> v2)
pub struct JobStatusChangedUpcaster;

impl JobStatusChangedUpcaster {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for JobStatusChangedUpcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcaster for JobStatusChangedUpcaster {
    fn from_version(&self) -> u32 {
        1
    }

    fn to_version(&self) -> u32 {
        JOB_EVENT_VERSION
    }

    fn event_type(&self) -> EventTypeId {
        job_event_types::JOB_STATUS_CHANGED
    }

    fn upcast(
        &self,
        _event_type: EventTypeId,
        _version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut result = payload.clone();
        result["_event_version"] = serde_json::json!(JOB_EVENT_VERSION);

        if !result.is_object() || !result.as_object().unwrap().contains_key("correlation_id") {
            result["correlation_id"] = serde_json::json!(None::<String>);
        }

        if !result.is_object() || !result.as_object().unwrap().contains_key("actor") {
            result["actor"] = serde_json::json!(None::<String>);
        }

        Ok(result)
    }
}

/// Upcaster for JobCancelled events (v1 -> v2)
pub struct JobCancelledUpcaster;

impl JobCancelledUpcaster {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for JobCancelledUpcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcaster for JobCancelledUpcaster {
    fn from_version(&self) -> u32 {
        1
    }

    fn to_version(&self) -> u32 {
        JOB_EVENT_VERSION
    }

    fn event_type(&self) -> EventTypeId {
        job_event_types::JOB_CANCELLED
    }

    fn upcast(
        &self,
        _event_type: EventTypeId,
        _version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut result = payload.clone();
        result["_event_version"] = serde_json::json!(JOB_EVENT_VERSION);

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
pub struct JobIdentityUpcaster {
    event_type: EventTypeId,
}

impl JobIdentityUpcaster {
    #[inline]
    pub fn new(event_type: &'static str) -> Self {
        Self {
            event_type: EventTypeId(event_type),
        }
    }
}

impl EventUpcaster for JobIdentityUpcaster {
    fn from_version(&self) -> u32 {
        JOB_EVENT_VERSION - 1
    }

    fn to_version(&self) -> u32 {
        JOB_EVENT_VERSION
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
        result["_event_version"] = serde_json::json!(JOB_EVENT_VERSION);
        Ok(result)
    }
}

// ============================================================================
// Upcaster Registry for Job Events
// ============================================================================

/// Registry containing all upcasters for job domain events
pub struct JobEventUpcasterRegistry {
    registry: EventUpcasterRegistry,
}

impl JobEventUpcasterRegistry {
    #[inline]
    pub fn new() -> Self {
        Self {
            registry: EventUpcasterRegistry::new(),
        }
    }
}

impl Default for JobEventUpcasterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl JobEventUpcasterRegistry {
    /// Register all job event upcasters
    pub fn register_all(&mut self) {
        // Version 1 -> 2 upcasters for events that may have old schema
        self.registry.register(JobCreatedUpcaster::new());
        self.registry.register(JobStatusChangedUpcaster::new());
        self.registry.register(JobCancelledUpcaster::new());

        // Identity upcasters for events that were already at v2 or later
        self.registry
            .register(JobIdentityUpcaster::new("job.retried"));
        self.registry
            .register(JobIdentityUpcaster::new("job.assigned"));
        self.registry
            .register(JobIdentityUpcaster::new("job.accepted"));
        self.registry
            .register(JobIdentityUpcaster::new("job.dispatch_acknowledged"));
        self.registry
            .register(JobIdentityUpcaster::new("job.run_job_received"));
        self.registry
            .register(JobIdentityUpcaster::new("job.queued"));
        self.registry
            .register(JobIdentityUpcaster::new("job.execution_error"));
        self.registry
            .register(JobIdentityUpcaster::new("job.dispatch_failed"));
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

pub use self::job_event_types::*;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::event::upcasting::EventTypeId;

    #[test]
    fn test_job_created_upcaster() {
        let upcaster = JobCreatedUpcaster::new();
        assert_eq!(upcaster.from_version(), 1);
        assert_eq!(upcaster.to_version(), JOB_EVENT_VERSION);
        assert_eq!(upcaster.event_type(), job_event_types::JOB_CREATED);

        let payload = serde_json::json!({
            "job_id": "job-123",
            "spec": {"command": "echo hello"},
            "occurred_at": "2024-01-01T00:00:00Z"
        });

        let result = upcaster
            .upcast(job_event_types::JOB_CREATED, 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], JOB_EVENT_VERSION);
        assert_eq!(result["job_id"], "job-123");
    }

    #[test]
    fn test_identity_upcaster() {
        let upcaster = JobIdentityUpcaster::new("job.retried");
        assert_eq!(upcaster.from_version(), 1);
        assert_eq!(upcaster.to_version(), JOB_EVENT_VERSION);

        let payload = serde_json::json!({
            "job_id": "job-456",
            "attempt": 2
        });

        let result = upcaster
            .upcast(EventTypeId("job.retried"), 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], JOB_EVENT_VERSION);
    }

    #[test]
    fn test_job_event_registry() {
        let mut registry = JobEventUpcasterRegistry::new();
        registry.register_all();

        assert!(!registry.is_empty());
        assert!(
            registry
                .registry()
                .has_upcaster(job_event_types::JOB_CREATED, 1)
        );
        assert!(
            registry
                .registry()
                .has_upcaster(job_event_types::JOB_STATUS_CHANGED, 1)
        );
    }

    #[test]
    fn test_upcast_job_created_event() {
        let mut registry = JobEventUpcasterRegistry::new();
        registry.register_all();

        let payload = serde_json::json!({
            "job_id": "job-789",
            "spec": {"command": "test"},
            "occurred_at": "2024-01-01T00:00:00Z"
        });

        let result = registry
            .upcast_payload(job_event_types::JOB_CREATED, 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], JOB_EVENT_VERSION);
        assert_eq!(result["job_id"], "job-789");
    }
}
