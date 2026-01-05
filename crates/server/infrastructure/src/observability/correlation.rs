//! Correlation Module - Propagates correlation IDs across the system
//!
//! Provides:
//! - Extraction of correlation IDs from headers
//! - Injection of correlation IDs into headers
//! - Propagation through the event-driven architecture
//!
//! EPIC-43: Sprint 5 - Observabilidad
//! US-EDA-502: Propagar correlation_id a headers NATS

use hodei_server_domain::outbox::OutboxEventView;
use hodei_server_domain::shared_kernel::CorrelationId;
use std::collections::HashMap;

/// NATS header names for correlation
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";
pub const TRACE_PARENT_HEADER: &str = "traceparent";
pub const TRACE_STATE_HEADER: &str = "tracestate";

/// Headers for NATS message propagation
#[derive(Debug, Default, Clone)]
pub struct NatsHeaders {
    /// Core correlation ID
    pub correlation_id: Option<String>,

    /// W3C traceparent header
    pub traceparent: Option<String>,

    /// W3C tracestate header
    pub tracestate: Option<String>,

    /// Custom headers
    pub custom: HashMap<String, String>,
}

impl NatsHeaders {
    /// Create empty headers
    pub fn new() -> Self {
        Self::default()
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: &str) -> Self {
        self.correlation_id = Some(correlation_id.to_string());
        self
    }

    /// Set traceparent
    pub fn with_traceparent(mut self, traceparent: &str) -> Self {
        self.traceparent = Some(traceparent.to_string());
        self
    }

    /// Set tracestate
    pub fn with_tracestate(mut self, tracestate: &str) -> Self {
        self.tracestate = Some(tracestate.to_string());
        self
    }

    /// Add custom header
    pub fn with_custom(mut self, key: &str, value: &str) -> Self {
        self.custom.insert(key.to_string(), value.to_string());
        self
    }
}

/// Extract correlation ID from event metadata
pub fn extract_correlation_id_from_event(event: &OutboxEventView) -> Option<String> {
    // Try to get from metadata
    if let Some(metadata) = &event.metadata {
        if let Some(corr_id) = metadata.get("correlation_id") {
            return corr_id.as_str().map(|s| s.to_string());
        }
    }

    // Fallback to aggregate ID if no correlation ID
    Some(event.aggregate_id.to_string())
}

/// Create NATS headers for publishing an event
pub fn create_event_headers(event: &OutboxEventView) -> NatsHeaders {
    let correlation_id = extract_correlation_id_from_event(event);

    NatsHeaders::new()
        .with_correlation_id(correlation_id.as_deref().unwrap_or(""))
        .with_custom("event_type", &event.event_type)
        .with_custom("aggregate_id", &event.aggregate_id.to_string())
        .with_custom("event_id", &event.id.to_string())
}

/// Middleware for adding correlation ID to gRPC request extensions
#[derive(Debug, Clone)]
pub struct CorrelationContext {
    /// Current correlation ID
    pub correlation_id: CorrelationId,

    /// Parent span ID for tracing
    pub parent_span_id: Option<String>,

    /// Trace state
    pub trace_state: Option<String>,
}

impl CorrelationContext {
    /// Create new context with generated correlation ID
    pub fn generate() -> Self {
        Self {
            correlation_id: CorrelationId::generate(),
            parent_span_id: None,
            trace_state: None,
        }
    }

    /// Create from existing correlation ID
    pub fn from_id(id: &str) -> Option<Self> {
        CorrelationId::from_string(id).map(|id| Self {
            correlation_id: id,
            parent_span_id: None,
            trace_state: None,
        })
    }
}

/// Convert context to NATS headers for propagation
pub fn context_to_headers(context: &CorrelationContext) -> NatsHeaders {
    NatsHeaders::new()
        .with_correlation_id(&context.correlation_id.to_string_value())
        .with_traceparent(context.parent_span_id.as_deref().unwrap_or(""))
        .with_tracestate(context.trace_state.as_deref().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nats_headers_creation() {
        let headers = NatsHeaders::new()
            .with_correlation_id("test-correlation")
            .with_traceparent("00-123-456-01")
            .with_custom("custom_key", "custom_value");

        assert_eq!(headers.correlation_id, Some("test-correlation".to_string()));
        assert_eq!(headers.traceparent, Some("00-123-456-01".to_string()));
        assert_eq!(
            headers.custom.get("custom_key"),
            Some(&"custom_value".to_string())
        );
    }

    #[test]
    fn test_extract_correlation_id_from_event() {
        let event = OutboxEventView {
            id: uuid::Uuid::new_v4(),
            aggregate_id: uuid::Uuid::new_v4(),
            aggregate_type: hodei_server_domain::outbox::AggregateType::Job,
            event_type: "JobCreated".to_string(),
            event_version: 1,
            payload: serde_json::json!({"test": "data"}),
            metadata: Some(serde_json::json!({
                "correlation_id": "test-correlation-id"
            })),
            idempotency_key: None,
            created_at: chrono::Utc::now(),
            published_at: None,
            status: hodei_server_domain::outbox::OutboxStatus::Pending,
            retry_count: 0,
            last_error: None,
        };

        let corr_id = extract_correlation_id_from_event(&event);
        assert_eq!(corr_id, Some("test-correlation-id".to_string()));
    }

    #[test]
    fn test_correlation_context_generation() {
        let context = CorrelationContext::generate();
        assert!(!context.correlation_id.as_str().is_empty());
    }

    #[test]
    fn test_correlation_context_from_id() {
        let context = CorrelationContext::from_id("a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        assert!(context.is_some());
        assert_eq!(
            context.unwrap().correlation_id.as_str(),
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
        );
    }
}
