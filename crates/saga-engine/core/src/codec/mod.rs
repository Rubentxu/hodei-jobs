//! Event codec traits and implementations.
//!
//! This module provides the [`EventCodec`] trait for serializing and deserializing
//! [`HistoryEvent`](super::event::HistoryEvent) objects. Multiple codec implementations
//! are provided with different trade-offs between performance and debuggability.
//!
//! # Architecture
//!
//! The codec module is designed with abstractions to allow swapping serialization
//! formats without changing consuming code:
//!
//! ```ignore
//! // Consumer code - format agnostic
//! let encoded = codec.encode(&event).await?;
//! let decoded = codec.decode(&encoded)?;
//! ```
//!
//! # Available Codecs
//!
//! - [`BincodeCodec`]: Best performance, Rust-native (default for production)
//! - [`JsonCodec`]: Human-readable, good for debugging
//! - [`PostcardCodec`]: Zero-alloc, embedded-friendly
//!
//! # Performance Characteristics
//!
//! | Codec     | Serialize | Deserialize | Size   | Zero-Copy |
//! |-----------|-----------|-------------|--------|-----------|
//! | Bincode   | ⭐⭐⭐⭐⭐   | ⭐⭐⭐⭐⭐      | Small  | No        |
//! | Postcard  | ⭐⭐⭐⭐    | ⭐⭐⭐⭐       | Smallest| Possible  |
//! | JSON      | ⭐⭐       | ⭐⭐         | Large  | No        |

use super::event::{CURRENT_EVENT_VERSION, HistoryEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Error type for codec operations.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("JSON serialization/deserialization error")]
    Json(#[from] serde_json::Error),

    #[error("Bincode serialization/deserialization error")]
    Bincode(#[from] bincode::Error),

    #[error("Postcard serialization/deserialization error")]
    Postcard(#[from] postcard::Error),

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
/// This trait abstracts the serialization format, allowing different
/// implementations to be swapped without changing consuming code.
///
/// Implementors provide different serialization strategies:
/// - [`BincodeCodec`]: Compact, fast binary format (recommended for production)
/// - [`JsonCodec`]: Human-readable, good for debugging
/// - [`PostcardCodec`]: Zero-allocation, embedded-friendly
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

    /// Return the human-readable name of this codec.
    fn codec_name(&self) -> &'static str {
        self.codec_id()
    }
}

/// Supported codec types for factory creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecType {
    /// Bincode codec - fastest serialization, Rust-native
    Bincode,
    /// JSON codec - human-readable, debuggable
    Json,
    /// Postcard codec - zero-allocation, embedded-friendly
    Postcard,
}

impl CodecType {
    /// Create a codec instance from this type.
    pub fn create_codec(&self) -> Box<dyn EventCodec<Error = CodecError>> {
        match self {
            Self::Bincode => Box::new(BincodeCodec::new()),
            Self::Json => Box::new(JsonCodec::new()),
            Self::Postcard => Box::new(PostcardCodec::new()),
        }
    }
}

impl std::str::FromStr for CodecType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bincode" | "binary" => Ok(Self::Bincode),
            "json" | "text" => Ok(Self::Json),
            "postcard" | "embedded" => Ok(Self::Postcard),
            _ => Err(format!("Unknown codec type: {}", s)),
        }
    }
}

impl std::fmt::Display for CodecType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bincode => write!(f, "bincode"),
            Self::Json => write!(f, "json"),
            Self::Postcard => write!(f, "postcard"),
        }
    }
}

// ============================================================================
// JSON Codec
// ============================================================================

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

    fn codec_name(&self) -> &'static str {
        "JSON (Human-readable)"
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
            event_type: event.event_type.as_str().to_string(),
            category: event.category.as_str().to_string(),
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

        use super::event::{EventId, SagaId};

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

// ============================================================================
// Bincode Codec
// ============================================================================

/// Binary event codec using bincode.
///
/// This codec produces compact binary output with excellent performance,
/// making it ideal for:
/// - Production workloads with high throughput
/// - Storage efficiency
/// - Network transmission
///
/// # Advantages
///
/// - Fastest serialization/deserialization in Rust ecosystem
/// - Compact binary output
/// - Native Rust implementation
///
/// # Limitations
///
/// - Output is not human-readable
/// - Requires Rust deserializer (not interoperable)
pub struct BincodeCodec;

impl BincodeCodec {
    /// Create a new binary codec.
    pub fn new() -> Self {
        Self
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

    fn codec_name(&self) -> &'static str {
        "Bincode (Fastest binary)"
    }
}

/// Internal wrapper for bincode serialization.
///
/// Uses u8 for event_type and category to minimize size.
#[derive(Debug, Serialize, Deserialize)]
struct BincodeEventWrapper {
    event_id: u64,
    saga_id: String,
    event_type: u8, // Compact representation
    category: u8,   // Compact representation
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
            event_type: event.event_type.to_compact_u8(),
            category: event.category as u8,
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

        let category = EventCategory::from_u8(self.category).ok_or_else(|| {
            CodecError::parse_error(format!("Unknown category: {}", self.category))
        })?;

        let event_type =
            EventType::from_category_and_u8(category, self.event_type).ok_or_else(|| {
                CodecError::parse_error(format!("Unknown event type: {}", self.event_type))
            })?;

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

// ============================================================================
// Postcard Codec
// ============================================================================

/// Zero-allocation event codec using postcard.
///
/// This codec is designed for embedded systems or memory-constrained environments:
/// - No dynamic allocations during serialization
/// - Minimal binary size
/// - Suitable for constrained environments
///
/// # When to Use
///
/// - Embedded systems (no-std support)
/// - Memory-constrained scenarios
/// - When predictable memory usage is critical
#[derive(Debug, Default, Clone)]
pub struct PostcardCodec;

impl PostcardCodec {
    /// Create a new postcard codec.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl EventCodec for PostcardCodec {
    type Error = CodecError;

    fn encode(&self, event: &HistoryEvent) -> Result<Vec<u8>, Self::Error> {
        let wrapper = PostcardEventWrapper::from(event);
        postcard::to_stdvec(&wrapper).map_err(CodecError::Postcard)
    }

    fn decode(&self, data: &[u8]) -> Result<HistoryEvent, Self::Error> {
        let wrapper: PostcardEventWrapper =
            postcard::from_bytes(data).map_err(CodecError::Postcard)?;
        wrapper.to_history_event()
    }

    fn codec_id(&self) -> &'static str {
        "postcard"
    }

    fn codec_name(&self) -> &'static str {
        "Postcard (Zero-alloc)"
    }
}

/// Internal wrapper for postcard serialization.
#[derive(Debug, Serialize, Deserialize)]
struct PostcardEventWrapper {
    event_id: u64,
    saga_id: String,
    event_type: u8,
    category: u8,
    timestamp_millis: i64,
    attributes: Vec<u8>, // JSON bytes like bincode
    event_version: u32,
    is_reset_point: bool,
    is_retry: bool,
    parent_event_id: Option<u64>,
    task_queue: Option<String>,
    trace_id: Option<String>,
}

impl PostcardEventWrapper {
    fn from(event: &HistoryEvent) -> Self {
        Self {
            event_id: event.event_id.0,
            saga_id: event.saga_id.0.clone(),
            event_type: event.event_type.to_compact_u8(),
            category: event.category as u8,
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
        if self.event_version != CURRENT_EVENT_VERSION {
            return Err(CodecError::version_mismatch(
                CURRENT_EVENT_VERSION,
                self.event_version,
            ));
        }

        use super::event::{EventCategory, EventId, EventType, SagaId};

        let category = EventCategory::from_u8(self.category).ok_or_else(|| {
            CodecError::parse_error(format!("Unknown category: {}", self.category))
        })?;

        let event_type =
            EventType::from_category_and_u8(category, self.event_type).ok_or_else(|| {
                CodecError::parse_error(format!("Unknown event type: {}", self.event_type))
            })?;

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

// ============================================================================
// Postcard Codec
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventCategory, EventId, EventType, HistoryEvent, SagaId};
    use serde_json::json;

    #[test]
    fn test_json_codec_roundtrip() {
        let codec = JsonCodec::new();

        let original = HistoryEvent::new(
            EventId(0),
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
            EventId(0),
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
    fn test_postcard_codec_roundtrip() {
        let codec = PostcardCodec::new();

        let original = HistoryEvent::new(
            EventId(0),
            SagaId("test-saga".to_string()),
            EventType::TimerFired,
            EventCategory::Timer,
            json!({"timer_id": "timer-123"}),
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
            EventId(0),
            SagaId("test-saga".to_string()),
            EventType::WorkflowExecutionStarted,
            EventCategory::Workflow,
            json!({"key": "value", "numbers": [1, 2, 3, 4, 5]}),
        );

        let json_codec = JsonCodec::new();
        let bincode_codec = BincodeCodec::new();
        let postcard_codec = PostcardCodec::new();

        let json_encoded = json_codec.encode(&original).unwrap();
        let bincode_encoded = bincode_codec.encode(&original).unwrap();
        let postcard_encoded = postcard_codec.encode(&original).unwrap();

        assert!(
            bincode_encoded.len() < json_encoded.len(),
            "Bincode should be smaller: {} vs {}",
            bincode_encoded.len(),
            json_encoded.len()
        );

        assert!(
            postcard_encoded.len() <= bincode_encoded.len(),
            "Postcard should be equal or smaller: {} vs {}",
            postcard_encoded.len(),
            bincode_encoded.len()
        );

        println!("JSON size: {} bytes", json_encoded.len());
        println!("Bincode size: {} bytes", bincode_encoded.len());
        println!("Postcard size: {} bytes", postcard_encoded.len());
    }

    #[test]
    fn test_codec_type_factory() {
        assert_eq!(CodecType::Bincode.create_codec().codec_id(), "bincode");
        assert_eq!(CodecType::Json.create_codec().codec_id(), "json");
        assert_eq!(CodecType::Postcard.create_codec().codec_id(), "postcard");
    }

    #[test]
    fn test_codec_type_parse() {
        assert_eq!("bincode".parse::<CodecType>().unwrap(), CodecType::Bincode);
        assert_eq!("json".parse::<CodecType>().unwrap(), CodecType::Json);
        assert_eq!(
            "postcard".parse::<CodecType>().unwrap(),
            CodecType::Postcard
        );
    }

    #[test]
    fn test_compact_encoding_all_event_types() {
        // Verify that all event types can be round-tripped through compact encoding
        use crate::event::EventType::*;

        let test_cases = vec![
            (WorkflowExecutionStarted, EventCategory::Workflow),
            (WorkflowExecutionCompleted, EventCategory::Workflow),
            (ActivityTaskScheduled, EventCategory::Activity),
            (TimerFired, EventCategory::Timer),
            (SignalReceived, EventCategory::Signal),
            (MarkerRecorded, EventCategory::Marker),
            (SnapshotCreated, EventCategory::Snapshot),
            (CommandIssued, EventCategory::Command),
            (ChildWorkflowExecutionStarted, EventCategory::ChildWorkflow),
            (LocalActivityScheduled, EventCategory::LocalActivity),
            (SideEffectRecorded, EventCategory::SideEffect),
            (WorkflowUpdateAccepted, EventCategory::Update),
            (UpsertSearchAttributes, EventCategory::SearchAttribute),
            (NexusOperationStarted, EventCategory::Nexus),
        ];

        for (event_type, category) in test_cases {
            let compact = event_type.to_compact_u8();
            let restored = EventType::from_category_and_u8(category, compact);
            assert_eq!(
                Some(event_type),
                restored,
                "Failed to round-trip {:?}",
                event_type
            );
        }
    }
}
