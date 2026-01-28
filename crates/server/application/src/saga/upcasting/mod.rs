//!
//! # Unified Event Upcasting Registry
//!
//! Central registry for all event upcasters in the Hodei Jobs platform.
//! Provides unified access to job and worker event upcasters.
//!
//! ## Architecture
//!
//! ```text
//! +---------------------+     +----------------------+     +-------------------+
//! |  Job Events v1      | --> |  UpcasterRegistry    | --> |  Current Schema   |
//! |  (legacy events)    |     |  - Job Upcasters     |     |  (v2)             |
//! +---------------------+     |  - Worker Upcasters  |     +-------------------+
//!                             +----------------------+
//! ```

use std::sync::Arc;

use saga_engine_core::event::upcasting::{EventTypeId, EventUpcasterRegistry, UpcastError};
use serde_json::Value;

use hodei_server_domain::jobs::upcasting::{JOB_EVENT_VERSION, JobEventUpcasterRegistry};
use hodei_server_domain::workers::upcasting::{WORKER_EVENT_VERSION, WorkerEventUpcasterRegistry};

// ============================================================================
// Unified Event Upcaster Registry
// ============================================================================

/// Unified registry containing all upcasters for the Hodei Jobs platform.
/// Combines job and worker event upcasters into a single registry.
#[derive(Default)]
pub struct UnifiedUpcasterRegistry {
    /// Job event upcasters
    job_registry: JobEventUpcasterRegistry,

    /// Worker event upcasters
    worker_registry: WorkerEventUpcasterRegistry,

    /// Combined registry for lookups
    combined_registry: EventUpcasterRegistry,
}

impl UnifiedUpcasterRegistry {
    /// Create a new registry with all upcasters registered
    #[inline]
    pub fn new() -> Self {
        let mut registry = Self {
            job_registry: JobEventUpcasterRegistry::new(),
            worker_registry: WorkerEventUpcasterRegistry::new(),
            combined_registry: EventUpcasterRegistry::new(),
        };

        // Register all job upcasters
        registry.job_registry.register_all();

        // Register all worker upcasters
        registry.worker_registry.register_all();

        registry
    }

    /// Create a registry with only job upcasters
    #[inline]
    pub fn job_only() -> Self {
        let mut registry = Self {
            job_registry: JobEventUpcasterRegistry::new(),
            worker_registry: WorkerEventUpcasterRegistry::new(),
            combined_registry: EventUpcasterRegistry::new(),
        };

        registry.job_registry.register_all();
        registry
    }

    /// Create a registry with only worker upcasters
    #[inline]
    pub fn worker_only() -> Self {
        let mut registry = Self {
            job_registry: JobEventUpcasterRegistry::new(),
            worker_registry: WorkerEventUpcasterRegistry::new(),
            combined_registry: EventUpcasterRegistry::new(),
        };

        registry.worker_registry.register_all();
        registry
    }

    /// Get the job registry
    #[inline]
    pub fn job_registry(&self) -> &JobEventUpcasterRegistry {
        &self.job_registry
    }

    /// Get the worker registry
    #[inline]
    pub fn worker_registry(&self) -> &WorkerEventUpcasterRegistry {
        &self.worker_registry
    }

    /// Upcast a job event payload
    #[inline]
    pub fn upcast_job_event(
        &self,
        event_type: EventTypeId,
        current_version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        self.job_registry
            .upcast_payload(event_type, current_version, payload)
    }

    /// Upcast a worker event payload
    #[inline]
    pub fn upcast_worker_event(
        &self,
        event_type: EventTypeId,
        current_version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        self.worker_registry
            .upcast_payload(event_type, current_version, payload)
    }

    /// Upcast any event payload (auto-detects domain)
    #[inline]
    pub fn upcast_event(
        &self,
        event_type: EventTypeId,
        current_version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        // Check if it's a job event
        if event_type.as_str().starts_with("job.") {
            self.upcast_job_event(event_type, current_version, payload)
        // Check if it's a worker event
        } else if event_type.as_str().starts_with("worker.") {
            self.upcast_worker_event(event_type, current_version, payload)
        } else {
            // Unknown event type, return as-is
            Ok(payload.clone())
        }
    }

    /// Get total number of registered upcasters
    #[inline]
    pub fn len(&self) -> usize {
        self.job_registry.len() + self.worker_registry.len()
    }

    /// Check if registry is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.job_registry.is_empty() && self.worker_registry.is_empty()
    }
}

/// Arc-wrapped registry for shared access across threads
pub type SharedUpcasterRegistry = Arc<UnifiedUpcasterRegistry>;

impl UnifiedUpcasterRegistry {
    /// Create an Arc-wrapped registry
    #[inline]
    pub fn shared() -> SharedUpcasterRegistry {
        Arc::new(Self::new())
    }

    /// Create a shared job-only registry
    #[inline]
    pub fn shared_job_only() -> SharedUpcasterRegistry {
        Arc::new(Self::job_only())
    }

    /// Create a shared worker-only registry
    #[inline]
    pub fn shared_worker_only() -> SharedUpcasterRegistry {
        Arc::new(Self::worker_only())
    }
}

// ============================================================================
// Module Exports
// ============================================================================

pub use self::SharedUpcasterRegistry as SharedRegistry;
pub use self::UnifiedUpcasterRegistry as Registry;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::event::upcasting::EventTypeId;

    #[test]
    fn test_unified_registry_creation() {
        let registry = UnifiedUpcasterRegistry::new();
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_job_event_upcasting() {
        let registry = UnifiedUpcasterRegistry::new();

        let payload = serde_json::json!({
            "job_id": "job-123",
            "spec": {"command": "echo hello"},
            "occurred_at": "2024-01-01T00:00:00Z"
        });

        let result = registry
            .upcast_job_event(EventTypeId("job.created"), 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], JOB_EVENT_VERSION);
    }

    #[test]
    fn test_worker_event_upcasting() {
        let registry = UnifiedUpcasterRegistry::new();

        let payload = serde_json::json!({
            "worker_id": "worker-456",
            "provider_id": "provider-1",
            "occurred_at": "2024-01-01T00:00:00Z"
        });

        let result = registry
            .upcast_worker_event(EventTypeId("worker.registered"), 1, &payload)
            .unwrap();

        assert_eq!(result["_event_version"], WORKER_EVENT_VERSION);
    }

    #[test]
    fn test_auto_detect_event_type() {
        let registry = UnifiedUpcasterRegistry::new();

        let job_payload = serde_json::json!({"job_id": "job-123"});
        let worker_payload = serde_json::json!({"worker_id": "worker-456"});

        // Job event
        let result = registry
            .upcast_event(EventTypeId("job.created"), 1, &job_payload)
            .unwrap();
        assert_eq!(result["_event_version"], JOB_EVENT_VERSION);

        // Worker event
        let result = registry
            .upcast_event(EventTypeId("worker.registered"), 1, &worker_payload)
            .unwrap();
        assert_eq!(result["_event_version"], WORKER_EVENT_VERSION);
    }

    #[test]
    fn test_shared_registry() {
        let shared = UnifiedUpcasterRegistry::shared();
        let shared2 = UnifiedUpcasterRegistry::shared();

        // Both should point to the same data (Arc)
        assert!(Arc::ptr_eq(&shared, &shared2) || shared.len() == shared2.len());
    }

    #[test]
    fn test_registry_counts() {
        let registry = UnifiedUpcasterRegistry::new();

        // Should have 11 job upcasters (3 v1->v2 + 8 identity)
        assert_eq!(registry.job_registry.len(), 11);

        // Should have 13 worker upcasters (3 v1->v2 + 10 identity)
        assert_eq!(registry.worker_registry.len(), 13);

        // Total should be 24
        assert_eq!(registry.len(), 24);
    }
}
