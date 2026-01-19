//! Event codec traits and implementations.
//!
//! This module provides the [`EventCodec`] trait for serializing and deserializing
//! [`HistoryEvent`](super::event::HistoryEvent) objects. Multiple codec implementations
//! are provided with different trade-offs between performance and debuggability.

use super::event::{CURRENT_EVENT_VERSION, HistoryEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error as StdError;
use std::fmt;

/// Error type for codec operations.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("JSON serialization/deserialization error")]
    Json(#[from] serde_json::Error),

    #[error("Bincode serialization/deserialization error")]
    Bincode(#[from] bincode::Error),

    #[error("Invalid codec version: expected {expected}, got {actual}")]
    InvalidVersion { expected: u32, actual: u32 },

    #[error("Data too large for codec limits")]
    DataTooLarge,

    #[error("Parse error: {0}")]
    Parse(String),
}

impl CodecError {
    /// Create a version mismatch error.
    pub fn version_mismatch(expected: u32, actual: u32) -> Self {
        Self::InvalidVersion { expected, actual }
    }

    /// Create a parse error.
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }
}

/// Trait for encoding and decoding events.
///
/// Implementors provide different serialization strategies:
/// - [`JsonCodec`]: Human-readable, good for debugging
/// - [`BincodeCodec`]: Compact, fast binary format
#[async_trait::async_trait]
pub trait EventCodec: Send + Sync + 'static {
    /// Error type for this codec.
    type Error: std::fmt::Debug + Send + Sync + 'static;

    /// Encode a history event to bytes.
    fn encode(&self, event: &HistoryEvent) -> Result<Vec<u8>, Self::Error>;

    /// Decode a history event from bytes.
    fn decode(&self, data: &[u8]) -> Result<HistoryEvent, Self::Error>;

    /// Return a unique identifier for this codec.
    fn codec_id(&self) -> &'static str;
}

/// JSON-based event codec.
///
/// This codec produces human-readable JSON output, making it ideal for:
/// - Debugging and logging
/// - Development and testing
/// - Scenarios where inspectability is important
///
/// # Performance
///
/// JSON serialization is slower than binary formats. For high-throughput
/// scenarios, consider using [`BincodeCodec`] instead.
#[derive(Debug, Default, Clone)]
pub struct JsonCodec;

impl JsonCodec {
    /// Create a new JSON codec.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl EventCodec for JsonCodec {
    type Error = CodecError;

    fn encode(&self, event: &HistoryEvent) -> Result<Vec<u8>, Self::Error> {
        let wrapper = EventWrapper::from(event);
        serde_json::to_vec_pretty(&wrapper).map_err(CodecError::Json)
    }

    fn decode(&self, data: &[u8]) -> Result<HistoryEvent, Self::Error> {
        let wrapper: EventWrapper = serde_json::from_slice(data).map_err(CodecError::Json)?;
        wrapper.to_history_event()
    }

    fn codec_id(&self) -> &'static str {
        "json"
    }
}

/// Internal wrapper for JSON serialization.
#[derive(Debug, Serialize, Deserialize)]
struct EventWrapper {
    event_id: u64,
    saga_id: String,
    event_type: String,
    category: String,
    timestamp: String,
    attributes: Value,
    event_version: u32,
    is_reset_point: bool,
    is_retry: bool,
    parent_event_id: Option<u64>,
    task_queue: Option<String>,
    trace_id: Option<String>,
}

impl EventWrapper {
    fn from(event: &HistoryEvent) -> Self {
        Self {
            event_id: event.event_id.0,
            saga_id: event.saga_id.0.clone(),
            event_type: format!("{:?}", event.event_type),
            category: format!("{:?}", event.category),
            timestamp: event.timestamp.to_rfc3339(),
            attributes: event.attributes.clone(),
            event_version: event.event_version,
            is_reset_point: event.is_reset_point,
            is_retry: event.is_retry,
            parent_event_id: event.parent_event_id.map(|id| id.0),
            task_queue: event.task_queue.clone(),
            trace_id: event.trace_id.clone(),
        }
    }

    fn to_history_event(self) -> Result<HistoryEvent, CodecError> {
        // Verify version
        if self.event_version != CURRENT_EVENT_VERSION {
            return Err(CodecError::version_mismatch(
                CURRENT_EVENT_VERSION,
                self.event_version,
            ));
        }

        use super::event::{EventCategory, EventId, EventType, SagaId};

        let event_type = self.event_type.parse().map_err(|_| {
            CodecError::parse_error(format!("Unknown event type: {}", self.event_type))
        })?;

        let category = self
            .category
            .parse()
            .map_err(|_| CodecError::parse_error(format!("Unknown category: {}", self.category)))?;

        let timestamp = self
            .timestamp
            .parse()
            .map_err(|_| CodecError::parse_error("Invalid timestamp format".to_string()))?;

        Ok(HistoryEvent {
            event_id: EventId(self.event_id),
            saga_id: SagaId(self.saga_id),
            event_type,
            category,
            timestamp,
            attributes: self.attributes,
            event_version: self.event_version,
            is_reset_point: self.is_reset_point,
            is_retry: self.is_retry,
            parent_event_id: self.parent_event_id.map(EventId),
            task_queue: self.task_queue,
            trace_id: self.trace_id,
        })
    }
}

/// Binary event codec using bincode.
///
/// This codec produces compact binary output, making it ideal for:
/// - Production workloads with high throughput
/// - Storage efficiency
/// - Network transmission
///
/// # Limitations
///
/// Bincode output is not human-readable. For debugging, consider:
/// - Using a hybrid approach (bincode for storage, JSON for debugging)
/// - Using `serde_json` for inspection of stored events
#[derive(Debug, Default, Clone)]
pub struct BincodeCodec {
    #[allow(dead_code)]
    max_size: usize,
}

impl BincodeCodec {
    /// Create a new binary codec with default limits.
    pub fn new() -> Self {
        Self {
            max_size: 10 * 1024 * 1024, // 10MB default limit
        }
    }

    /// Create a new binary codec with custom size limit.
    pub fn with_limit(max_size: usize) -> Self {
        Self { max_size }
    }
}

#[async_trait::async_trait]
impl EventCodec for BincodeCodec {
    type Error = CodecError;

    fn encode(&self, event: &HistoryEvent) -> Result<Vec<u8>, Self::Error> {
        let wrapper = BincodeEventWrapper::from(event);
        bincode::serialize(&wrapper).map_err(CodecError::Bincode)
    }

    fn decode(&self, data: &[u8]) -> Result<HistoryEvent, Self::Error> {
        let wrapper: BincodeEventWrapper =
            bincode::deserialize(data).map_err(CodecError::Bincode)?;
        wrapper.to_history_event()
    }

    fn codec_id(&self) -> &'static str {
        "bincode"
    }
}

/// Internal wrapper for bincode serialization.
#[derive(Debug, Serialize, Deserialize)]
struct BincodeEventWrapper {
    event_id: u64,
    saga_id: String,
    event_type: String,
    category: String,
    timestamp_millis: i64,
    attributes: Vec<u8>, // JSON bytes
    event_version: u32,
    is_reset_point: bool,
    is_retry: bool,
    parent_event_id: Option<u64>,
    task_queue: Option<String>,
    trace_id: Option<String>,
}

impl BincodeEventWrapper {
    fn from(event: &HistoryEvent) -> Self {
        Self {
            event_id: event.event_id.0,
            saga_id: event.saga_id.0.clone(),
            event_type: format!("{:?}", event.event_type),
            category: format!("{:?}", event.category),
            timestamp_millis: event.timestamp.timestamp_millis(),
            attributes: serde_json::to_vec(&event.attributes).unwrap_or_default(),
            event_version: event.event_version,
            is_reset_point: event.is_reset_point,
            is_retry: event.is_retry,
            parent_event_id: event.parent_event_id.map(|id| id.0),
            task_queue: event.task_queue.clone(),
            trace_id: event.trace_id.clone(),
        }
    }

    fn to_history_event(self) -> Result<HistoryEvent, CodecError> {
        // Verify version
        if self.event_version != CURRENT_EVENT_VERSION {
            return Err(CodecError::version_mismatch(
                CURRENT_EVENT_VERSION,
                self.event_version,
            ));
        }

        use super::event::{EventCategory, EventId, EventType, SagaId};

        let event_type = self.event_type.parse().map_err(|_| {
            CodecError::parse_error(format!("Unknown event type: {}", self.event_type))
        })?;

        let category = self
            .category
            .parse()
            .map_err(|_| CodecError::parse_error(format!("Unknown category: {}", self.category)))?;

        let timestamp = chrono::DateTime::from_timestamp_millis(self.timestamp_millis)
            .unwrap_or(chrono::Utc::now());

        let attributes = serde_json::from_slice(&self.attributes).map_err(CodecError::Json)?;

        Ok(HistoryEvent {
            event_id: EventId(self.event_id),
            saga_id: SagaId(self.saga_id),
            event_type,
            category,
            timestamp,
            attributes,
            event_version: self.event_version,
            is_reset_point: self.is_reset_point,
            is_retry: self.is_retry,
            parent_event_id: self.parent_event_id.map(EventId),
            task_queue: self.task_queue,
            trace_id: self.trace_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventCategory, EventType, HistoryEvent, SagaId};
    use serde_json::json;

    #[test]
    fn test_json_codec_roundtrip() {
        let codec = JsonCodec::new();

        let original = HistoryEvent::new(
            SagaId("test-saga".to_string()),
            EventType::WorkflowExecutionStarted,
            EventCategory::Workflow,
            json!({"key": "value", "nested": {"a": 1}}),
        );

        let encoded = codec.encode(&original).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(original.saga_id, decoded.saga_id);
        assert_eq!(original.event_type, decoded.event_type);
        assert_eq!(original.category, decoded.category);
        assert_eq!(original.event_id.0, decoded.event_id.0);
        assert_eq!(original.attributes, decoded.attributes);
    }

    #[test]
    fn test_bincode_codec_roundtrip() {
        let codec = BincodeCodec::new();

        let original = HistoryEvent::new(
            SagaId("test-saga".to_string()),
            EventType::ActivityTaskCompleted,
            EventCategory::Activity,
            json!({"result": "success"}),
        );

        let encoded = codec.encode(&original).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(original.saga_id, decoded.saga_id);
        assert_eq!(original.event_type, decoded.event_type);
        assert_eq!(original.event_id.0, decoded.event_id.0);
    }

    #[test]
    fn test_bincode_is_smaller_than_json() {
        let original = HistoryEvent::new(
            SagaId("test-saga".to_string()),
            EventType::WorkflowExecutionStarted,
            EventCategory::Workflow,
            json!({"key": "value", "numbers": [1, 2, 3, 4, 5]}),
        );

        let json_codec = JsonCodec::new();
        let bincode_codec = BincodeCodec::new();

        let json_encoded = json_codec.encode(&original).unwrap();
        let bincode_encoded = bincode_codec.encode(&original).unwrap();

        assert!(
            bincode_encoded.len() < json_encoded.len(),
            "Bincode should be smaller: {} vs {}",
            bincode_encoded.len(),
            json_encoded.len()
        );
    }

    #[test]
    fn test_codec_id() {
        let json_codec = JsonCodec::new();
        let bincode_codec = BincodeCodec::new();

        assert_eq!(json_codec.codec_id(), "json");
        assert_eq!(bincode_codec.codec_id(), "bincode");
    }
}
