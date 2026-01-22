//! # Production Integration Examples
//!
//! This module provides examples and integration points for using
//! the SagaContext migration module in production code.
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use hodei_server_domain::saga::context_migration::{create_saga_context, MigrationConfig};
//! use hodei_server_bin::config::ServerConfig;
//!
//! // In your application code where you create sagas:
//! fn create_provisioning_saga(
//!     saga_id: &SagaId,
//!     config: &ServerConfig,
//! ) -> Result<SagaContextEither, Error> {
//!     let context = create_saga_context(
//!         saga_id,
//!         SagaType::Provisioning,
//!         Some("corr-123".to_string()),
//!         Some("user-1".to_string()),
//!         config, // ServerConfig implements MigrationConfig
//!     );
//!
//!     // Handle both versions
//!     match context {
//!         SagaContextEither::V1(ctx) => {
//!             // Use legacy SagaContext
//!             Ok(SagaContextEither::V1(ctx))
//!         }
//!         SagaContextEither::V2(ctx) => {
//!             // Use new SagaContextV2
//!             Ok(SagaContextEither::V2(ctx))
//!         }
//!     }
//! }
//! ```

use crate::saga::{
    SagaId, SagaType,
    context_factory::SagaContextEither,
    context_migration::{MigrationConfig, WithSagaContext},
};

/// Example: Creating a saga context with feature flags
///
/// This function demonstrates how to integrate the migration module
/// into production code that creates saga contexts.
pub fn create_saga_with_migration(
    saga_id: &SagaId,
    saga_type: SagaType,
    correlation_id: Option<String>,
    actor: Option<String>,
    config: &dyn MigrationConfig,
) -> SagaContextEither {
    // The factory automatically uses the feature flag to decide V1 vs V2
    crate::saga::context_migration::create_saga_context(
        saga_id,
        saga_type,
        correlation_id,
        actor,
        config,
    )
}

/// Example: Processing a saga context polymorphically
///
/// This function demonstrates how to write code that works
/// with both V1 and V2 contexts using the SagaContextOps trait.
pub fn process_saga_context<F, R>(context: &SagaContextEither, processor: F) -> R
where
    F: FnOnce(&dyn crate::saga::context_migration::SagaContextOps) -> R,
{
    context.with_context(processor)
}

/// Example: Getting saga information regardless of version
///
/// This function shows how to extract common information
/// from either V1 or V2 context.
pub fn get_saga_info(context: &SagaContextEither) -> SagaInfo {
    context.with_context(|ctx| SagaInfo {
        id: ctx.saga_id().to_string(),
        type_: ctx.saga_type().as_str().to_string(),
        correlation_id: ctx.correlation_id().map(|s| s.to_string()),
        actor: ctx.actor().map(|s| s.to_string()),
        version: ctx.version(),
    })
}

/// Information about a saga, extracted from either V1 or V2 context
#[derive(Debug, Clone)]
pub struct SagaInfo {
    pub id: String,
    pub type_: String,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub version: u64,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_saga_with_migration_v1() {
        let config = MockConfig {
            v2_enabled: false,
            v2_percentage: 0,
        };
        let saga_id = SagaId::new();

        let result = create_saga_with_migration(
            &saga_id,
            SagaType::Provisioning,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
            &config,
        );

        match result {
            SagaContextEither::V1(ctx) => {
                assert_eq!(ctx.saga_id, saga_id);
            }
            SagaContextEither::V2(_) => panic!("Expected V1"),
        }
    }

    #[test]
    fn test_create_saga_with_migration_v2() {
        let config = MockConfig {
            v2_enabled: true,
            v2_percentage: 100,
        };
        let saga_id = SagaId::new();

        let result = create_saga_with_migration(
            &saga_id,
            SagaType::Provisioning,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
            &config,
        );

        match result {
            SagaContextEither::V2(ctx) => {
                assert_eq!(ctx.identity.id, saga_id);
            }
            SagaContextEither::V1(_) => panic!("Expected V2"),
        }
    }

    #[test]
    fn test_process_saga_context_polymorphically() {
        let config = MockConfig {
            v2_enabled: true,
            v2_percentage: 100,
        };
        let saga_id = SagaId::new();

        let context =
            create_saga_with_migration(&saga_id, SagaType::Execution, None, None, &config);

        // Process regardless of version
        let saga_type = process_saga_context(&context, |ctx| ctx.saga_type().clone());
        assert_eq!(saga_type, SagaType::Execution);
    }

    #[test]
    fn test_get_saga_info() {
        let config = MockConfig {
            v2_enabled: false,
            v2_percentage: 0,
        };
        let saga_id = SagaId::new();

        let context = create_saga_with_migration(
            &saga_id,
            SagaType::Provisioning,
            Some("corr-123".to_string()),
            Some("user-1".to_string()),
            &config,
        );

        let info = get_saga_info(&context);
        assert_eq!(info.id, saga_id.to_string());
        assert_eq!(info.type_, "PROVISIONING");
        assert_eq!(info.correlation_id, Some("corr-123".to_string()));
        assert_eq!(info.actor, Some("user-1".to_string()));
    }
}
