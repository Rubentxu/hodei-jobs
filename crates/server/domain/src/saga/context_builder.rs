//! SagaContext Builder
//!
//! Unified builder for creating saga contexts.
//! Post-migration: V2 is now the default and only implementation.
//!
//! ## Migration Complete
//!
//! As of Phase 5 cleanup, SagaContextV2 is now the sole implementation.
//! The V1 code has been removed and feature flags are no longer needed.

use crate::saga::{
    SagaId, SagaType,
    saga_context::{
        Actor, CorrelationId, DefaultSagaMetadata, SagaContextV2, SagaContextV2Builder,
    },
    types::SagaContext,
};
use chrono::Utc;

/// Creates a new saga context using V2 implementation
///
/// This is the primary entry point for creating saga contexts.
/// After Phase 5 migration, V2 is the default and only implementation.
///
/// # Arguments
///
/// * `saga_id` - Unique identifier for the saga
/// * `saga_type` - Type of saga being executed
/// * `correlation_id` - Optional correlation ID for distributed tracing
/// * `actor` - Optional actor who initiated the saga
///
/// # Returns
///
/// A new SagaContext with V2 implementation
///
/// # Example
///
/// ```ignore
/// use hodei_server_domain::saga::{create_saga_context, SagaId, SagaType};
///
/// let saga_id = SagaId::new();
/// let context = create_saga_context(
///     &saga_id,
///     SagaType::Provisioning,
///     Some("corr-123".to_string()),
///     Some("user-1".to_string()),
/// );
/// ```
pub fn create_saga_context(
    saga_id: &SagaId,
    saga_type: SagaType,
    correlation_id: Option<String>,
    actor: Option<String>,
) -> SagaContext {
    // Build V2 context
    let mut builder = SagaContextV2Builder::new()
        .with_id(saga_id.clone())
        .with_type(saga_type);

    if let Some(corr_id) = correlation_id {
        builder = builder.with_correlation_id(CorrelationId::new(corr_id));
    }

    if let Some(act) = actor {
        builder = builder.with_actor(Actor::new(act));
    }

    let ctx_v2 = builder
        .with_metadata(DefaultSagaMetadata)
        .build()
        .expect("SagaContextV2Builder should not fail with valid inputs");

    // Convert V2 to the unified SagaContext type
    ctx_v2.to_context()
}

/// Creates a saga context from persisted data
///
/// This is used when loading saga state from the database.
/// After Phase 5 migration, all persisted contexts are V2.
///
/// # Arguments
///
/// * `saga_id` - The unique saga identifier
/// * `saga_type` - The type of saga
/// * `correlation_id` - Optional correlation ID
/// * `actor` - Optional actor
/// * `started_at` - When the saga started
/// * `current_step` - Current step number
/// * `is_compensating` - Whether the saga is compensating
/// * `metadata` - Custom metadata
/// * `error_message` - Optional error message
/// * `version` - Optimistic locking version
/// * `trace_parent` - Optional W3C trace context
///
/// # Returns
///
/// A SagaContext with data from persistence
pub fn create_saga_context_from_persistence(
    saga_id: SagaId,
    saga_type: SagaType,
    correlation_id: Option<String>,
    actor: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    current_step: usize,
    is_compensating: bool,
    metadata: std::collections::HashMap<String, serde_json::Value>,
    error_message: Option<String>,
    version: u64,
    trace_parent: Option<String>,
) -> SagaContext {
    let ctx_v2 = SagaContextV2::<DefaultSagaMetadata>::from_persistence(
        saga_id,
        saga_type,
        correlation_id,
        actor,
        started_at,
        current_step,
        is_compensating,
        metadata,
        error_message,
        version,
        trace_parent,
    );

    ctx_v2.to_context()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_saga_context_basic() {
        let saga_id = SagaId::new();
        let context = create_saga_context(&saga_id, SagaType::Provisioning, None, None);

        assert_eq!(context.saga_id, saga_id);
        assert_eq!(context.saga_type, SagaType::Provisioning);
    }

    #[test]
    fn test_create_saga_context_with_correlation_and_actor() {
        let saga_id = SagaId::new();
        let context = create_saga_context(
            &saga_id,
            SagaType::Execution,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
        );

        assert_eq!(context.saga_id, saga_id);
        assert_eq!(context.saga_type, SagaType::Execution);
        assert_eq!(context.correlation_id, Some("corr-123".to_string()));
        assert_eq!(context.actor, Some("user-1".to_string()));
    }

    #[test]
    fn test_create_saga_context_from_persistence() {
        let saga_id = SagaId::new();
        let started_at = Utc::now();
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("key".to_string(), serde_json::json!("value"));

        let context = create_saga_context_from_persistence(
            saga_id.clone(),
            SagaType::Recovery,
            Some("corr-456".to_string()),
            Some("user-2".to_string()),
            started_at,
            3,
            false,
            metadata,
            None,
            5,
            Some("trace-123".to_string()),
        );

        assert_eq!(context.saga_id, saga_id);
        assert_eq!(context.saga_type, SagaType::Recovery);
        assert_eq!(context.current_step, 3);
        assert_eq!(context.version, 5);
        assert_eq!(context.trace_parent, Some("trace-123".to_string()));
    }
}
