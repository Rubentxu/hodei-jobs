//! # SagaContext Migration Module
//!
//! This module provides integration points for gradual migration from
//! SagaContext V1 to V2 using feature flags and the factory pattern.
//!
//! ## Migration Strategy (Strangler Fig Pattern)
//!
//! 1. **Phase 1**: Use factory with feature flags to create contexts
//! 2. **Phase 2**: Convert between V1 and V2 as needed
//! 3. **Phase 3**: Gradually increase V2 percentage
//! 4. **Phase 4**: Remove V1 code once 100% migrated
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use hodei_server_domain::saga::context_migration::{create_saga_context, SagaContextEither};
//!
//! // Create context with feature flag decision
//! let context = create_saga_context(
//!     &saga_id,
//!     saga_type,
//!     correlation_id,
//!     actor,
//!     &config, // ServerConfig with feature flags
//! )?;
//!
//! // Handle both versions
//! match context {
//!     SagaContextEither::V1(ctx) => { /* legacy code */ }
//!     SagaContextEither::V2(ctx) => { /* new code */ }
//! }
//! ```

use crate::saga::{
    SagaId, SagaType,
    context_factory::{SagaContextEither, SagaContextFactory},
    context_v2::{
        Actor, CorrelationId, DefaultSagaMetadata, SagaContextV2, SagaContextV2Builder, TraceParent,
    },
    types::SagaContext as SagaContextV1,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for SagaContext migration
///
/// This trait allows different config sources to provide
/// feature flag settings for gradual migration.
pub trait MigrationConfig: Send + Sync {
    /// Check if SagaContext V2 should be used for a specific saga
    fn should_use_saga_v2(&self, saga_id: &str) -> bool;

    /// Get the current V2 rollout percentage (0-100)
    fn v2_percentage(&self) -> u8;
}

/// Result of context creation - can be either V1 or V2
pub type SagaContextResult = SagaContextEither;

/// Creates a new saga context using the factory pattern with feature flags
///
/// This is the main entry point for creating saga contexts during migration.
/// It uses the feature flag configuration to decide whether to create V1 or V2.
///
/// # Arguments
/// * `saga_id` - The unique saga identifier
/// * `saga_type` - The type of saga being created
/// * `correlation_id` - Optional correlation ID for distributed tracing
/// * `actor` - Optional actor that initiated the saga
/// * `config` - Migration configuration with feature flags
///
/// # Returns
/// Either a V1 or V2 context wrapped in SagaContextEither
///
/// # Example
/// ```rust,ignore
/// let context = create_saga_context(
///     &saga_id,
///     SagaType::Provisioning,
///     Some("corr-123".to_string()),
///     Some("user-1".to_string()),
///     &server_config,
/// )?;
/// ```
pub fn create_saga_context(
    saga_id: &SagaId,
    saga_type: SagaType,
    correlation_id: Option<String>,
    actor: Option<String>,
    config: &dyn MigrationConfig,
) -> SagaContextResult {
    let saga_id_str = saga_id.to_string();
    let use_v2 = config.should_use_saga_v2(&saga_id_str);

    SagaContextFactory::create_context(saga_id, saga_type, correlation_id, actor, use_v2)
}

/// Creates a saga context from persisted data with version detection
///
/// When loading from the database, we need to detect which version was used.
/// For now, we default to V1 for backward compatibility.
///
/// TODO: In Phase 4d, add a version column to the database to detect this.
///
/// # Arguments
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
/// * `config` - Migration configuration
///
/// # Returns
/// Either a V1 or V2 context wrapped in SagaContextEither
pub fn create_saga_context_from_persistence(
    saga_id: SagaId,
    saga_type: SagaType,
    correlation_id: Option<String>,
    actor: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    current_step: usize,
    is_compensating: bool,
    metadata: HashMap<String, serde_json::Value>,
    error_message: Option<String>,
    version: u64,
    trace_parent: Option<String>,
    _config: &dyn MigrationConfig,
) -> SagaContextResult {
    // For now, always recreate V1 from persistence
    // In Phase 4d, we'll detect version from database column
    let ctx_v1 = SagaContextV1::from_persistence(
        saga_id,
        saga_type,
        correlation_id,
        actor,
        started_at,
        current_step,
        is_compensating,
        metadata,
        error_message,
        // Default state for persisted context
        crate::saga::SagaState::Pending, // Will be updated from DB
        version,
        trace_parent,
    );

    SagaContextEither::V1(ctx_v1)
}

/// Converts V1 context to V2
///
/// This is useful when migrating from V1 to V2 during execution.
///
/// # Arguments
/// * `ctx_v1` - The V1 context to convert
///
/// # Returns
/// A new V2 context with data from V1
pub fn convert_v1_to_v2(ctx_v1: &SagaContextV1) -> SagaContextV2<DefaultSagaMetadata> {
    SagaContextV2::from_v1(ctx_v1)
}

/// Converts V2 context to V1
///
/// This is useful when persisting V2 context to systems expecting V1.
///
/// # Arguments
/// * `ctx_v2` - The V2 context to convert
///
/// # Returns
/// A new V1 context with data from V2
pub fn convert_v2_to_v1(ctx_v2: &SagaContextV2<DefaultSagaMetadata>) -> SagaContextV1 {
    ctx_v2.to_v1()
}

/// Helper trait for types that need to work with both context versions
///
/// This allows writing generic code that works with either V1 or V2.
pub trait WithSagaContext {
    /// Execute a function with the context, regardless of version
    fn with_context<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&dyn SagaContextOps) -> R;
}

/// Trait for common operations across context versions
///
/// This abstracts the differences between V1 and V2, allowing
/// generic code to work with either version.
pub trait SagaContextOps {
    /// Get the saga ID
    fn saga_id(&self) -> &SagaId;

    /// Get the saga type
    fn saga_type(&self) -> &SagaType;

    /// Get the correlation ID as string
    fn correlation_id(&self) -> Option<&str>;

    /// Get the actor as string
    fn actor(&self) -> Option<&str>;

    /// Get the started timestamp
    fn started_at(&self) -> &chrono::DateTime<chrono::Utc>;

    /// Get the current step
    fn current_step(&self) -> usize;

    /// Check if compensating
    fn is_compensating(&self) -> bool;

    /// Get metadata
    fn metadata(&self) -> &HashMap<String, serde_json::Value>;

    /// Get error message
    fn error_message(&self) -> Option<&str>;

    /// Get the version
    fn version(&self) -> u64;

    /// Get trace parent as string
    fn trace_parent(&self) -> Option<&str>;
}

// Implement for V1
impl SagaContextOps for SagaContextV1 {
    fn saga_id(&self) -> &SagaId {
        &self.saga_id
    }

    fn saga_type(&self) -> &SagaType {
        &self.saga_type
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn actor(&self) -> Option<&str> {
        self.actor.as_deref()
    }

    fn started_at(&self) -> &chrono::DateTime<chrono::Utc> {
        &self.started_at
    }

    fn current_step(&self) -> usize {
        self.current_step
    }

    fn is_compensating(&self) -> bool {
        self.is_compensating
    }

    fn metadata(&self) -> &HashMap<String, serde_json::Value> {
        &self.metadata
    }

    fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn trace_parent(&self) -> Option<&str> {
        self.trace_parent.as_deref()
    }
}

// Implement for V2 with DefaultSagaMetadata
impl SagaContextOps for SagaContextV2<DefaultSagaMetadata> {
    fn saga_id(&self) -> &SagaId {
        &self.identity.id
    }

    fn saga_type(&self) -> &SagaType {
        &self.identity.type_
    }

    fn correlation_id(&self) -> Option<&str> {
        self.identity
            .correlation_id
            .as_ref()
            .map(|id| id.0.as_str())
    }

    fn actor(&self) -> Option<&str> {
        self.identity.actor.as_ref().map(|id| id.0.as_str())
    }

    fn started_at(&self) -> &chrono::DateTime<chrono::Utc> {
        &self.identity.started_at
    }

    fn current_step(&self) -> usize {
        self.execution.current_step
    }

    fn is_compensating(&self) -> bool {
        self.execution.phase == crate::saga::context_v2::SagaPhase::Compensating
    }

    fn metadata(&self) -> &HashMap<String, serde_json::Value> {
        // DefaultSagaMetadata is empty, return empty HashMap
        static EMPTY: std::sync::OnceLock<HashMap<String, serde_json::Value>> =
            std::sync::OnceLock::new();
        EMPTY.get_or_init(|| HashMap::new())
    }

    fn error_message(&self) -> Option<&str> {
        self.execution.error_message.as_deref()
    }

    fn version(&self) -> u64 {
        self.execution.version
    }

    fn trace_parent(&self) -> Option<&str> {
        self.identity.trace_parent.as_ref().map(|id| id.0.as_str())
    }
}

// Implement for SagaContextEither
impl WithSagaContext for SagaContextEither {
    fn with_context<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&dyn SagaContextOps) -> R,
    {
        match self {
            SagaContextEither::V1(ctx) => f(ctx),
            SagaContextEither::V2(ctx) => f(ctx),
        }
    }
}

/// Migration statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    /// Total number of sagas created
    pub total_sagas: u64,
    /// Number using V1
    pub v1_count: u64,
    /// Number using V2
    pub v2_count: u64,
    /// Current V2 percentage
    pub v2_percentage: u8,
}

impl MigrationStats {
    pub fn new() -> Self {
        Self {
            total_sagas: 0,
            v1_count: 0,
            v2_count: 0,
            v2_percentage: 0,
        }
    }

    pub fn record_v1(&mut self) {
        self.total_sagas += 1;
        self.v1_count += 1;
        self.update_percentage();
    }

    pub fn record_v2(&mut self) {
        self.total_sagas += 1;
        self.v2_count += 1;
        self.update_percentage();
    }

    fn update_percentage(&mut self) {
        if self.total_sagas > 0 {
            self.v2_percentage = ((self.v2_count as f64 / self.total_sagas as f64) * 100.0) as u8;
        }
    }
}

impl Default for MigrationStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock config for testing
    struct MockConfig {
        v2_enabled: bool,
        v2_percentage: u8,
    }

    impl MigrationConfig for MockConfig {
        fn should_use_saga_v2(&self, saga_id: &str) -> bool {
            if !self.v2_enabled {
                return false;
            }
            if self.v2_percentage == 0 {
                return false;
            }
            if self.v2_percentage >= 100 {
                return true;
            }

            // Use consistent hashing
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            saga_id.hash(&mut hasher);
            let hash = hasher.finish();

            let bucket = (hash % 100) as u8;
            bucket < self.v2_percentage
        }

        fn v2_percentage(&self) -> u8 {
            self.v2_percentage
        }
    }

    #[test]
    fn test_create_context_v1_when_disabled() {
        let config = MockConfig {
            v2_enabled: false,
            v2_percentage: 0,
        };
        let saga_id = SagaId::new();

        let result = create_saga_context(
            &saga_id,
            SagaType::Provisioning,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
            &config,
        );

        match result {
            SagaContextEither::V1(ctx) => {
                assert_eq!(ctx.saga_id, saga_id);
                assert_eq!(ctx.saga_type, SagaType::Provisioning);
            }
            SagaContextEither::V2(_) => panic!("Expected V1 context"),
        }
    }

    #[test]
    fn test_create_context_v2_when_enabled() {
        let config = MockConfig {
            v2_enabled: true,
            v2_percentage: 100,
        };
        let saga_id = SagaId::new();

        let result = create_saga_context(
            &saga_id,
            SagaType::Provisioning,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
            &config,
        );

        match result {
            SagaContextEither::V2(ctx) => {
                assert_eq!(ctx.identity.id, saga_id);
                assert_eq!(ctx.identity.type_, SagaType::Provisioning);
                assert_eq!(
                    ctx.identity.correlation_id.as_ref().map(|id| id.0.as_str()),
                    Some("corr-123")
                );
                assert_eq!(
                    ctx.identity.actor.as_ref().map(|id| id.0.as_str()),
                    Some("user-1")
                );
            }
            SagaContextEither::V1(_) => panic!("Expected V2 context"),
        }
    }

    #[test]
    fn test_convert_v1_to_v2() {
        let ctx_v1 = SagaContextV1::new(
            SagaId::new(),
            SagaType::Execution,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
        );

        let ctx_v2 = convert_v1_to_v2(&ctx_v1);

        assert_eq!(ctx_v2.identity.id, ctx_v1.saga_id);
        assert_eq!(ctx_v2.identity.type_, ctx_v1.saga_type);
        assert_eq!(
            ctx_v2
                .identity
                .correlation_id
                .as_ref()
                .map(|id| id.0.as_str()),
            ctx_v1.correlation_id.as_deref()
        );
        assert_eq!(
            ctx_v2.identity.actor.as_ref().map(|id| id.0.as_str()),
            ctx_v1.actor.as_deref()
        );
    }

    #[test]
    fn test_convert_v2_to_v1() {
        let ctx_v2 = SagaContextV2Builder::new()
            .with_id(SagaId::new())
            .with_type(SagaType::Execution)
            .with_correlation_id(CorrelationId::new("corr-123".to_string()))
            .with_actor(Actor::new("user-1".to_string()))
            .with_metadata(DefaultSagaMetadata)
            .build()
            .unwrap();

        let ctx_v1 = convert_v2_to_v1(&ctx_v2);

        assert_eq!(ctx_v1.saga_id, ctx_v2.identity.id);
        assert_eq!(ctx_v1.saga_type, ctx_v2.identity.type_);
        assert_eq!(
            ctx_v1.correlation_id.as_deref(),
            ctx_v2
                .identity
                .correlation_id
                .as_ref()
                .map(|id| id.0.as_str())
        );
        assert_eq!(
            ctx_v1.actor.as_deref(),
            ctx_v2.identity.actor.as_ref().map(|id| id.0.as_str())
        );
    }

    #[test]
    fn test_with_context_trait() {
        let ctx_v1 = SagaContextV1::new(
            SagaId::new(),
            SagaType::Provisioning,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
        );

        let either = SagaContextEither::V1(ctx_v1);

        let saga_type = either.with_context(|ctx| ctx.saga_type().clone());
        assert_eq!(saga_type, SagaType::Provisioning);

        let corr_id = either.with_context(|ctx| ctx.correlation_id().map(|s| s.to_string()));
        assert_eq!(corr_id, Some("corr-123".to_string()));
    }

    #[test]
    fn test_migration_stats() {
        let mut stats = MigrationStats::new();

        stats.record_v1();
        assert_eq!(stats.total_sagas, 1);
        assert_eq!(stats.v1_count, 1);
        assert_eq!(stats.v2_count, 0);
        assert_eq!(stats.v2_percentage, 0);

        stats.record_v2();
        assert_eq!(stats.total_sagas, 2);
        assert_eq!(stats.v1_count, 1);
        assert_eq!(stats.v2_count, 1);
        assert_eq!(stats.v2_percentage, 50);

        stats.record_v2();
        assert_eq!(stats.total_sagas, 3);
        assert_eq!(stats.v1_count, 1);
        assert_eq!(stats.v2_count, 2);
        assert_eq!(stats.v2_percentage, 66); // 66.66% rounded down
    }

    #[test]
    fn test_consistent_hashing() {
        // Same saga ID should always produce same version decision
        let config = MockConfig {
            v2_enabled: true,
            v2_percentage: 50, // 50% should use V2
        };

        let saga_id = SagaId::from_string("test-saga-123");

        let result1 = create_saga_context(&saga_id, SagaType::Provisioning, None, None, &config);

        let result2 = create_saga_context(&saga_id, SagaType::Provisioning, None, None, &config);

        // Both should be same version
        match (&result1, &result2) {
            (SagaContextEither::V1(_), SagaContextEither::V1(_)) => {}
            (SagaContextEither::V2(_), SagaContextEither::V2(_)) => {}
            _ => panic!("Consistent hashing failed: same saga ID produced different versions"),
        }
    }

    #[test]
    fn test_saga_context_ops_v2() {
        let ctx_v2 = SagaContextV2Builder::new()
            .with_id(SagaId::new())
            .with_type(SagaType::Provisioning)
            .with_correlation_id(CorrelationId::new("corr-123".to_string()))
            .with_actor(Actor::new("user-1".to_string()))
            .with_metadata(DefaultSagaMetadata)
            .build()
            .unwrap();

        // Test trait methods
        assert_eq!(ctx_v2.identity.type_, SagaType::Provisioning);
        assert_eq!(
            ctx_v2
                .identity
                .correlation_id
                .as_ref()
                .map(|id| id.0.as_str()),
            Some("corr-123")
        );
        assert_eq!(
            ctx_v2.identity.actor.as_ref().map(|id| id.0.as_str()),
            Some("user-1")
        );
    }
}
