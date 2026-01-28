//!
//! # Event Upcasting
//!
//! Schema evolution for event sourcing. Allows old events to be read
//! as new schema versions without modifying stored events.
//!

use serde_json::Value;

/// Current event schema version
pub const CURRENT_EVENT_VERSION: u32 = 2;

/// Upcast error types
#[derive(Debug, thiserror::Error)]
pub enum UpcastError {
    #[error("Missing upcaster for event type {event_type} from version {from_version}")]
    MissingUpcaster {
        event_type: &'static str,
        from_version: u32,
    },

    #[error("Deserialization failed during upcasting")]
    DeserializationFailed,

    #[error("Serialization failed during upcasting")]
    SerializationFailed,

    #[error("Upcasting chain broken: cannot upgrade from {from} to {to}")]
    ChainBroken { from: u32, to: u32 },
}

/// Event type identifier for registry lookups
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventTypeId(pub &'static str);

impl EventTypeId {
    #[inline]
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl From<&'static str> for EventTypeId {
    fn from(s: &'static str) -> Self {
        Self(s)
    }
}

/// Event upcaster trait for schema evolution
pub trait EventUpcaster: Send + Sync {
    /// Get the source version this upcaster handles
    fn from_version(&self) -> u32;

    /// Get the target version this upcaster produces
    fn to_version(&self) -> u32;

    /// Get the event type this upcaster handles
    fn event_type(&self) -> EventTypeId;

    /// Transform an event from the source version to the target version
    fn upcast(
        &self,
        event_type: EventTypeId,
        version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError>;
}

/// Boxed upcaster for storage in registry
pub type BoxedUpcaster = Box<dyn EventUpcaster>;

/// High-performance upcaster registry
pub struct EventUpcasterRegistry {
    /// Upcasters indexed by (event_type, from_version)
    upcasters: Vec<(EventTypeId, u32, BoxedUpcaster)>,
}

impl Default for EventUpcasterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EventUpcasterRegistry {
    /// Create a new registry with no upcasters
    pub fn new() -> Self {
        Self {
            upcasters: Vec::new(),
        }
    }

    /// Register an upcaster
    pub fn register<U: EventUpcaster + 'static>(&mut self, upcaster: U) {
        self.upcasters.push((
            upcaster.event_type(),
            upcaster.from_version(),
            Box::new(upcaster),
        ));
    }

    /// Get an upcaster for a specific event type and version
    #[inline]
    pub fn get(&self, event_type: EventTypeId, from_version: u32) -> Option<&BoxedUpcaster> {
        self.upcasters
            .iter()
            .find(|(et, v, _)| *et == event_type && *v == from_version)
            .map(|(_, _, u)| u)
    }

    /// Check if an upcaster exists
    #[inline]
    pub fn has_upcaster(&self, event_type: EventTypeId, from_version: u32) -> bool {
        self.upcasters
            .iter()
            .any(|(et, v, _)| *et == event_type && *v == from_version)
    }

    /// Upcast an event payload to the current version
    pub fn upcast_payload(
        &self,
        event_type: EventTypeId,
        current_version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        let mut version = current_version;
        let mut result = payload.clone();
        let mut iterations = 0;
        const MAX_ITERATIONS: u32 = 10;

        while version < CURRENT_EVENT_VERSION {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                return Err(UpcastError::ChainBroken {
                    from: version,
                    to: CURRENT_EVENT_VERSION,
                });
            }

            match self.get(event_type, version) {
                Some(upcaster) => {
                    result = upcaster.upcast(event_type, version, &result)?;
                    version = upcaster.to_version();
                }
                None => {
                    return Err(UpcastError::MissingUpcaster {
                        event_type: event_type.as_str(),
                        from_version: version,
                    });
                }
            }
        }

        Ok(result)
    }

    /// Get the number of registered upcasters
    #[inline]
    pub fn len(&self) -> usize {
        self.upcasters.len()
    }

    /// Check if the registry is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.upcasters.is_empty()
    }

    /// Clear all upcasters
    #[inline]
    pub fn clear(&mut self) {
        self.upcasters.clear();
    }
}

/// Unified event payload wrapper
/// Stores event data without requiring Serialize/Deserialize derive
#[derive(Debug, Clone)]
pub struct UnifiedEvent {
    /// Unique event identifier
    pub event_id: super::EventId,

    /// Saga this event belongs to
    pub saga_id: super::SagaId,

    /// Aggregate this event affects (None for saga-only events)
    pub aggregate_id: Option<String>,

    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,

    /// Trace ID for observability
    pub trace_id: Option<String>,

    /// The event type identifier
    pub event_type_id: EventTypeId,

    /// The event payload (JSON)
    pub payload: Value,

    /// Saga-specific metadata (None for domain events)
    pub saga_metadata: Option<SagaMetadata>,

    /// Schema version for upcasting
    pub schema_version: u32,
}

/// Metadata for saga-specific events
#[derive(Debug, Clone, Default)]
pub struct SagaMetadata {
    /// Step that generated this event
    pub step_id: Option<String>,

    /// Whether this is a compensation event
    pub is_compensation: bool,

    /// Event ID this compensates
    pub compensation_for: Option<super::EventId>,
}

impl UnifiedEvent {
    /// Create a new domain event (no saga metadata)
    pub fn new_domain_event(
        event_id: super::EventId,
        saga_id: super::SagaId,
        event_type_id: EventTypeId,
        payload: Value,
        aggregate_id: Option<String>,
    ) -> Self {
        Self {
            event_id,
            saga_id,
            aggregate_id,
            correlation_id: None,
            trace_id: None,
            event_type_id,
            payload,
            saga_metadata: None,
            schema_version: CURRENT_EVENT_VERSION,
        }
    }

    /// Create a new saga event (no aggregate)
    pub fn new_saga_event(
        event_id: super::EventId,
        saga_id: super::SagaId,
        event_type_id: EventTypeId,
        payload: Value,
        step_id: Option<String>,
        is_compensation: bool,
    ) -> Self {
        Self {
            event_id,
            saga_id,
            aggregate_id: None,
            correlation_id: None,
            trace_id: None,
            event_type_id,
            payload,
            saga_metadata: Some(SagaMetadata {
                step_id,
                is_compensation,
                compensation_for: None,
            }),
            schema_version: CURRENT_EVENT_VERSION,
        }
    }

    /// Check if this is a domain event
    #[inline]
    pub fn is_domain_event(&self) -> bool {
        self.aggregate_id.is_some()
    }

    /// Check if this is a saga event
    #[inline]
    pub fn is_saga_event(&self) -> bool {
        self.saga_metadata.is_some()
    }

    /// Upcast this event to the current schema version
    pub fn upcast_to_current(&self, registry: &EventUpcasterRegistry) -> Result<Self, UpcastError> {
        let new_payload =
            registry.upcast_payload(self.event_type_id, self.schema_version, &self.payload)?;

        Ok(Self {
            event_id: self.event_id,
            saga_id: self.saga_id.clone(),
            aggregate_id: self.aggregate_id.clone(),
            correlation_id: self.correlation_id.clone(),
            trace_id: self.trace_id.clone(),
            event_type_id: self.event_type_id,
            payload: new_payload,
            saga_metadata: self.saga_metadata.clone(),
            schema_version: CURRENT_EVENT_VERSION,
        })
    }
}

/// Simple upcaster implementation for common cases
pub struct SimpleUpcaster<F>
where
    F: Fn(EventTypeId, u32, &Value) -> Result<Value, UpcastError> + Send + Sync,
{
    from_version: u32,
    to_version: u32,
    event_type: EventTypeId,
    transform: F,
}

impl<F> SimpleUpcaster<F>
where
    F: Fn(EventTypeId, u32, &Value) -> Result<Value, UpcastError> + Send + Sync + 'static,
{
    /// Create a new simple upcaster
    pub fn new(from_version: u32, to_version: u32, event_type: &'static str, transform: F) -> Self {
        Self {
            from_version,
            to_version,
            event_type: EventTypeId(event_type),
            transform,
        }
    }
}

impl<F> EventUpcaster for SimpleUpcaster<F>
where
    F: Fn(EventTypeId, u32, &Value) -> Result<Value, UpcastError> + Send + Sync + 'static,
{
    fn from_version(&self) -> u32 {
        self.from_version
    }

    fn to_version(&self) -> u32 {
        self.to_version
    }

    fn event_type(&self) -> EventTypeId {
        self.event_type
    }

    fn upcast(
        &self,
        event_type: EventTypeId,
        version: u32,
        payload: &Value,
    ) -> Result<Value, UpcastError> {
        (self.transform)(event_type, version, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestUpcaster;

    impl EventUpcaster for TestUpcaster {
        fn from_version(&self) -> u32 {
            1
        }
        fn to_version(&self) -> u32 {
            2
        }
        fn event_type(&self) -> EventTypeId {
            EventTypeId("order.created")
        }

        fn upcast(
            &self,
            _event_type: EventTypeId,
            _version: u32,
            payload: &Value,
        ) -> Result<Value, UpcastError> {
            let mut result = payload.clone();
            result["upcasted"] = serde_json::json!(true);
            result["new_field"] = serde_json::json!("added in v2");
            Ok(result)
        }
    }

    #[test]
    fn test_upcaster_registration() {
        let mut registry = EventUpcasterRegistry::new();
        registry.register(TestUpcaster);

        assert!(registry.has_upcaster(EventTypeId("order.created"), 1));
        assert!(!registry.has_upcaster(EventTypeId("order.created"), 2));
        assert!(!registry.has_upcaster(EventTypeId("order.updated"), 1));
    }

    #[test]
    fn test_upcast_payload() {
        let mut registry = EventUpcasterRegistry::new();
        registry.register(TestUpcaster);

        let payload = serde_json::json!({"order_id": "123", "total": 100});
        let result = registry
            .upcast_payload(EventTypeId("order.created"), 1, &payload)
            .unwrap();

        assert_eq!(result["upcasted"], true);
        assert_eq!(result["new_field"], "added in v2");
        assert_eq!(result["order_id"], "123");
    }

    #[test]
    fn test_missing_upcaster() {
        let registry = EventUpcasterRegistry::new();

        let payload = serde_json::json!({"order_id": "123"});
        let result = registry.upcast_payload(EventTypeId("order.updated"), 1, &payload);

        assert!(matches!(result, Err(UpcastError::MissingUpcaster { .. })));
    }

    #[test]
    fn test_event_creation() {
        let event = UnifiedEvent::new_domain_event(
            super::super::EventId(1),
            super::super::SagaId::new(),
            EventTypeId("order.created"),
            serde_json::json!({"total": 100}),
            Some("order-123".to_string()),
        );

        assert!(event.is_domain_event());
        assert!(!event.is_saga_event());
        assert_eq!(event.aggregate_id, Some("order-123".to_string()));
    }

    #[test]
    fn test_simple_upcaster() {
        let mut registry = EventUpcasterRegistry::new();

        let upcaster = SimpleUpcaster::new(1, 2, "test.event", |_, _, payload| {
            let mut result = payload.clone();
            result["simple_upcast"] = serde_json::json!(true);
            Ok(result)
        });

        registry.register(upcaster);

        let payload = serde_json::json!({"data": "test"});
        let result = registry
            .upcast_payload(EventTypeId("test.event"), 1, &payload)
            .unwrap();

        assert_eq!(result["simple_upcast"], true);
    }

    #[test]
    fn test_event_upcast() {
        let mut registry = EventUpcasterRegistry::new();
        registry.register(TestUpcaster);

        // Create event with old schema version (1) to trigger upcasting
        let mut event = UnifiedEvent::new_domain_event(
            super::super::EventId(1),
            super::super::SagaId::new(),
            EventTypeId("order.created"),
            serde_json::json!({"order_id": "123"}),
            Some("order-123".to_string()),
        );
        event.schema_version = 1; // Force old version

        let upcasted = event.upcast_to_current(&registry).unwrap();

        assert_eq!(upcasted.schema_version, CURRENT_EVENT_VERSION);
        assert_eq!(upcasted.payload["upcasted"], true);
    }

    #[test]
    fn test_chain_upcasting() {
        let mut registry = EventUpcasterRegistry::new();

        // Chain: v0→v1, v1→v2 (CURRENT=2)
        let upcaster_0_to_1 = SimpleUpcaster::new(0, 1, "chained.event", |_, _, payload| {
            let mut result = payload.clone();
            result["step0"] = serde_json::json!(true);
            Ok(result)
        });

        let upcaster_1_to_2 = SimpleUpcaster::new(1, 2, "chained.event", |_, _, payload| {
            let mut result = payload.clone();
            result["step1"] = serde_json::json!(true);
            Ok(result)
        });

        registry.register(upcaster_0_to_1);
        registry.register(upcaster_1_to_2);

        let payload = serde_json::json!({"original": "data"});
        // Start from version 0 to trigger full chain
        let result = registry
            .upcast_payload(EventTypeId("chained.event"), 0, &payload)
            .unwrap();

        assert_eq!(result["step0"], true);
        assert_eq!(result["step1"], true);
        assert_eq!(result["original"], "data");
    }
}
