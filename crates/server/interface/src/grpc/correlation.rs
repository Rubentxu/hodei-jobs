//! Correlation ID Propagation
//!
//! Provides automatic extraction and generation of correlation IDs for request tracing.
//! Ensures end-to-end traceability across the system.
//!
//! This module implements US-30.4: Automatic correlation_id Propagation
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    CorrelationIdManager                          │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
//! │  │ Extractor   │─►│ Generator   │─►│ Propagator              │  │
//! │  │ (headers)   │  │ (UUID)      │  │ (context)               │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────┘
//!          │                    │                     │
//!          ▼                    ▼                     ▼
//!    ┌────────────┐      ┌────────────┐      ┌────────────────────┐
//!    │ gRPC       │      │ UUID       │      │ Domain Events      │
//!    │ Request    │      │ Generator  │      │ (audit trail)      │
//!    └────────────┘      └────────────┘      └────────────────────┘
//! ```

use hodei_server_domain::events::EventMetadata;
use hodei_server_domain::shared_kernel::CorrelationId;
use std::future::Future;
use tonic::{Request, Status};
use tracing::{Level, Span, debug, info};

/// Header name for correlation ID
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";

/// Context key for storing correlation ID in request extensions
pub const CORRELATION_ID_CONTEXT_KEY: &str = "correlation_id";

/// Result of correlation ID extraction/generation
#[derive(Debug, Clone)]
pub struct CorrelationIdResult {
    /// The correlation ID (either extracted or generated)
    pub correlation_id: CorrelationId,
    /// Whether the correlation ID was extracted from headers
    pub was_extracted: bool,
    /// Whether a new correlation ID was generated
    pub was_generated: bool,
}

impl CorrelationIdResult {
    /// Creates a new result for an extracted correlation ID
    pub fn extracted(correlation_id: CorrelationId) -> Self {
        Self {
            correlation_id,
            was_extracted: true,
            was_generated: false,
        }
    }

    /// Creates a new result for a generated correlation ID
    pub fn generated(correlation_id: CorrelationId) -> Self {
        Self {
            correlation_id,
            was_extracted: false,
            was_generated: true,
        }
    }

    /// Returns the correlation ID as a string
    pub fn as_str(&self) -> String {
        self.correlation_id.as_str()
    }
}

/// Manages correlation ID extraction, generation, and propagation.
///
/// This struct provides functionality to:
/// - Extract correlation IDs from gRPC request headers
/// - Generate new correlation IDs when not present
/// - Store correlation IDs in request extensions for propagation
///
/// # Connascence Analysis
///
/// **Before Refactoring:**
/// - Connascence of Position (CoP): correlation_id passed as parameter everywhere
/// - Connascence of Meaning (CoM): Different string formats for correlation_id
///
/// **After Refactoring:**
/// - Connascence of Type (CoT): Centralized `CorrelationId` value object
/// - Connascence of Name (CoN): Clear methods for extraction/generation
#[derive(Debug, Clone, Default)]
pub struct CorrelationIdManager {}

impl CorrelationIdManager {
    /// Creates a new CorrelationIdManager
    pub fn new() -> Self {
        Self {}
    }

    /// Extracts correlation ID from gRPC request headers.
    ///
    /// If the header is present, extracts its value.
    /// If not present, generates a new UUID.
    ///
    /// # Arguments
    ///
    /// * `request` - The gRPC request to extract from
    ///
    /// # Returns
    ///
    /// A `CorrelationIdResult` containing the correlation ID and metadata
    ///
    /// # Example
    ///
    /// ```ignore
    /// let manager = CorrelationIdManager::new();
    /// let result = manager.extract_from_request(&request);
    /// debug!("Correlation ID: {}", result.correlation_id);
    /// ```
    pub fn extract_from_request<T>(&self, request: &Request<T>) -> CorrelationIdResult {
        // Try to extract from header
        if let Some(value) = request.metadata().get(CORRELATION_ID_HEADER) {
            if let Ok(correlation_id_str) = value.to_str() {
                if !correlation_id_str.is_empty() {
                    if let Some(correlation_id) = CorrelationId::from_string(correlation_id_str) {
                        debug!("Extracted correlation_id from header: {}", correlation_id);
                        return CorrelationIdResult::extracted(correlation_id);
                    }
                }
            }
        }

        // Generate new correlation ID
        let correlation_id = CorrelationId::generate();
        debug!("Generated new correlation_id: {}", correlation_id);
        CorrelationIdResult::generated(correlation_id)
    }

    /// Extracts correlation ID from a hashmap of headers.
    ///
    /// Useful for non-gRPC contexts where headers are stored in a map.
    ///
    /// # Arguments
    ///
    /// * `headers` - HashMap of header name to value
    ///
    /// # Returns
    ///
    /// A `CorrelationIdResult`
    pub fn extract_from_headers(
        &self,
        headers: &std::collections::HashMap<String, String>,
    ) -> CorrelationIdResult {
        // Try to extract from header
        if let Some(value) = headers.get(CORRELATION_ID_HEADER) {
            if !value.is_empty() {
                if let Some(correlation_id) = CorrelationId::from_string(value.as_str()) {
                    debug!("Extracted correlation_id from headers: {}", correlation_id);
                    return CorrelationIdResult::extracted(correlation_id);
                }
            }
        }

        // Generate new correlation ID
        let correlation_id = CorrelationId::generate();
        debug!("Generated new correlation_id: {}", correlation_id);
        CorrelationIdResult::generated(correlation_id)
    }

    /// Generates a new correlation ID.
    ///
    /// # Returns
    ///
    /// A new `CorrelationId`
    pub fn generate(&self) -> CorrelationId {
        CorrelationId::generate()
    }

    /// Stores correlation ID in request extensions for downstream use.
    ///
    /// # Arguments
    ///
    /// * `request` - The gRPC request to modify
    /// * `correlation_id` - The correlation ID to store
    pub fn store_in_request<T>(&self, request: &mut Request<T>, correlation_id: &CorrelationId) {
        request.extensions_mut().insert(correlation_id.clone());
    }

    /// Retrieves correlation ID from request extensions.
    ///
    /// # Arguments
    ///
    /// * `request` - The gRPC request to extract from
    ///
    /// # Returns
    ///
    /// Option containing the correlation ID if present
    pub fn get_from_request<T>(request: &Request<T>) -> Option<CorrelationId> {
        request.extensions().get::<CorrelationId>().cloned()
    }

    /// Creates event metadata with the given correlation ID.
    ///
    /// # Arguments
    ///
    /// * `correlation_id` - The correlation ID to include
    /// * `actor` - Optional actor identifier
    ///
    /// # Returns
    ///
    /// `EventMetadata` with the correlation ID set
    pub fn create_event_metadata(
        &self,
        correlation_id: &CorrelationId,
        actor: Option<&str>,
    ) -> EventMetadata {
        EventMetadata::for_system_event(
            Some(correlation_id.to_string_value()),
            actor.unwrap_or("system"),
        )
    }

    /// Creates a tracing span with correlation ID context.
    ///
    /// # Arguments
    ///
    /// * `correlation_id` - The correlation ID for the span
    /// * `name` - Name of the span
    /// * `level` - Tracing level for the span
    ///
    /// # Returns
    ///
    /// A new `Span` with correlation ID context
    pub fn create_span(
        &self,
        correlation_id: &CorrelationId,
        _name: &'static str,
        _level: Level,
    ) -> Span {
        let span = Span::current();
        let correlation_id_str = correlation_id.to_string_value();
        span.record("correlation_id", correlation_id_str.as_str());
        span
    }
}

/// Middleware function that extracts correlation ID from request and stores it.
///
/// This function can be used as gRPC interceptor or middleware.
pub async fn correlation_id_middleware<T, F, R>(
    request: Request<T>,
    next: impl Fn(Request<T>) -> F,
) -> Result<R, Status>
where
    F: Future<Output = Result<R, Status>>,
{
    let manager = CorrelationIdManager::new();
    let result = manager.extract_from_request(&request);

    // Create a mutable copy for storing
    let mut request = request;

    // Store correlation ID in extensions
    manager.store_in_request(&mut request, &result.correlation_id);

    // Log the correlation ID
    info!(
        "Request processed with correlation_id: {} (extracted: {}, generated: {})",
        result.correlation_id, result.was_extracted, result.was_generated
    );

    // Call the next handler
    next(request).await
}

/// Extension trait for adding correlation ID methods to Request
pub trait RequestCorrelationExt {
    /// Gets the correlation ID from this request
    fn correlation_id(&self) -> Option<CorrelationId>;

    /// Gets or generates a correlation ID for this request
    fn correlation_id_or_generate(&self) -> CorrelationId;
}

impl<T> RequestCorrelationExt for Request<T> {
    fn correlation_id(&self) -> Option<CorrelationId> {
        CorrelationIdManager::get_from_request(self)
    }

    fn correlation_id_or_generate(&self) -> CorrelationId {
        self.correlation_id()
            .unwrap_or_else(|| CorrelationId::generate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tonic::metadata::{MetadataKey, MetadataMap};
    use uuid::Uuid;

    #[test]
    fn test_extract_correlation_id_from_header() {
        let manager = CorrelationIdManager::new();
        let mut metadata = MetadataMap::new();
        // Use a valid UUID format
        let key = MetadataKey::from_bytes(CORRELATION_ID_HEADER.as_bytes()).unwrap();
        metadata.insert(key, "a1b2c3d4-e5f6-7890-abcd-ef1234567890".parse().unwrap());

        // Create request with metadata
        let mut request = Request::new(());
        *request.metadata_mut() = metadata;
        let result = manager.extract_from_request(&request);

        assert!(result.was_extracted);
        assert!(!result.was_generated);
        assert_eq!(result.as_str(), "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    }

    #[test]
    fn test_generate_correlation_id_when_missing() {
        let manager = CorrelationIdManager::new();
        let request = Request::new(());

        let result = manager.extract_from_request(&request);

        assert!(!result.was_extracted);
        assert!(result.was_generated);
        assert!(!result.as_str().is_empty());

        // Verify it's a valid UUID
        let uuid_result = Uuid::parse_str(&result.as_str());
        assert!(uuid_result.is_ok());
    }

    #[test]
    fn test_extract_correlation_id_from_headers_map() {
        let manager = CorrelationIdManager::new();
        let mut headers = HashMap::new();
        headers.insert(
            CORRELATION_ID_HEADER.to_string(),
            "b2c3d4e5-f6a1-890b-cdef-1234567890ab".to_string(),
        );

        let result = manager.extract_from_headers(&headers);

        assert!(result.was_extracted);
        assert_eq!(result.as_str(), "b2c3d4e5-f6a1-890b-cdef-1234567890ab");
    }

    #[test]
    fn test_generate_correlation_id_from_empty_headers() {
        let manager = CorrelationIdManager::new();
        let headers = HashMap::new();

        let result = manager.extract_from_headers(&headers);

        assert!(!result.was_extracted);
        assert!(result.was_generated);
    }

    #[test]
    fn test_correlation_id_result_extracted() {
        let correlation_id = CorrelationId::from_string("a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        let result = CorrelationIdResult::extracted(correlation_id.clone().unwrap());

        assert!(result.was_extracted);
        assert!(!result.was_generated);
        assert_eq!(result.as_str(), "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    }

    #[test]
    fn test_correlation_id_result_generated() {
        let correlation_id = CorrelationId::generate();
        let result = CorrelationIdResult::generated(correlation_id.clone());

        assert!(!result.was_extracted);
        assert!(result.was_generated);
        assert_eq!(result.as_str(), correlation_id.as_str());
    }

    #[test]
    fn test_store_and_get_from_request() {
        let manager = CorrelationIdManager::new();
        let mut request = Request::new(());
        let correlation_id = CorrelationId::generate();

        manager.store_in_request(&mut request, &correlation_id);
        let extracted = CorrelationIdManager::get_from_request(&request);

        assert!(extracted.is_some());
        assert_eq!(extracted.unwrap().as_str(), correlation_id.as_str());
    }

    #[test]
    fn test_request_correlation_ext() {
        let request = Request::new(());

        // Initially no correlation ID
        assert!(request.correlation_id().is_none());

        // Should generate one
        let correlation_id = request.correlation_id_or_generate();
        assert!(!correlation_id.as_str().is_empty());
    }

    #[test]
    fn test_create_event_metadata() {
        let manager = CorrelationIdManager::new();
        let correlation_id = CorrelationId::generate();

        let metadata = manager.create_event_metadata(&correlation_id, Some("test-actor"));

        assert!(metadata.correlation_id.is_some());
        assert_eq!(metadata.correlation_id.unwrap(), correlation_id.as_str());
        assert_eq!(metadata.actor, Some("test-actor".to_string()));
    }
}
