//! SagaContext Factory
//!
//! Factory module for creating SagaContext instances with feature flag support.
//! This enables gradual migration from V1 to V2 using the Strangler Fig pattern.

use crate::saga::{
    SagaId, SagaType,
    context_v2::{DefaultSagaMetadata, SagaContextV2, SagaMetadata},
    types::SagaContext as SagaContextV1,
};

/// Factory for creating saga contexts with feature flag support
///
/// This factory determines whether to use V1 or V2 based on runtime configuration,
/// enabling gradual migration and A/B testing.
///
/// # Example
///
/// ```ignore
/// use hodei_server_domain::saga::{SagaContextFactory, SagaId, SagaType};
///
/// let saga_id = SagaId::new();
/// let use_v2 = true;
///
/// let context = SagaContextFactory::create_context(
///     &saga_id,
///     SagaType::Provisioning,
///     None,
///     None,
///     use_v2
/// );
/// ```
pub struct SagaContextFactory;

impl SagaContextFactory {
    /// Create a saga context, automatically choosing V1 or V2 based on feature flag
    ///
    /// # Arguments
    ///
    /// * `saga_id` - Unique identifier for the saga
    /// * `saga_type` - Type of saga being executed
    /// * `correlation_id` - Optional correlation ID for distributed tracing
    /// * `actor` - Optional actor who initiated the saga
    /// * `use_v2` - Whether to use V2 (feature flag)
    ///
    /// # Returns
    ///
    /// * `Some(Box<dyn Any>)` - Boxed context (either V1 or V2)
    /// * `None` - If context creation fails
    pub fn create_context(
        saga_id: &SagaId,
        saga_type: SagaType,
        correlation_id: Option<String>,
        actor: Option<String>,
        use_v2: bool,
    ) -> SagaContextEither {
        if use_v2 {
            // Create V2 context
            let mut identity_builder =
                crate::saga::context_v2::SagaIdentity::new(saga_id.clone(), saga_type);

            if let Some(corr_id) = correlation_id {
                identity_builder = identity_builder
                    .with_correlation_id(crate::saga::context_v2::CorrelationId::new(corr_id));
            }

            if let Some(act) = actor {
                identity_builder =
                    identity_builder.with_actor(crate::saga::context_v2::Actor::new(act));
            }

            let v2_context = SagaContextV2::<DefaultSagaMetadata> {
                identity: identity_builder,
                execution: crate::saga::context_v2::SagaExecutionState::new(),
                outputs: crate::saga::context_v2::StepOutputs::new(),
                metadata: DefaultSagaMetadata,
            };

            SagaContextEither::V2(v2_context)
        } else {
            // Create V1 context
            let v1_context = SagaContextV1::new(saga_id.clone(), saga_type, correlation_id, actor);

            SagaContextEither::V1(v1_context)
        }
    }
}

/// Enum wrapper for either V1 or V2 saga context
///
/// This enables dynamic context selection at runtime while maintaining type safety.
pub enum SagaContextEither {
    /// V1 (legacy) context
    V1(SagaContextV1),
    /// V2 (refactored) context with default metadata
    V2(SagaContextV2<DefaultSagaMetadata>),
}

impl SagaContextEither {
    /// Get the saga ID (works for both V1 and V2)
    pub fn saga_id(&self) -> &SagaId {
        match self {
            SagaContextEither::V1(ctx) => &ctx.saga_id,
            SagaContextEither::V2(ctx) => ctx.id(),
        }
    }

    /// Get the saga type (works for both V1 and V2)
    pub fn saga_type(&self) -> SagaType {
        match self {
            SagaContextEither::V1(ctx) => ctx.saga_type,
            SagaContextEither::V2(ctx) => *ctx.type_(),
        }
    }

    /// Check if this is a V2 context
    pub fn is_v2(&self) -> bool {
        matches!(self, SagaContextEither::V2(_))
    }

    /// Convert to V1 if this is V2 (no-op if already V1)
    pub fn to_v1(&self) -> SagaContextV1 {
        match self {
            SagaContextEither::V1(ctx) => ctx.clone(),
            SagaContextEither::V2(ctx) => ctx.to_v1(),
        }
    }

    /// Get reference to V1 context (panics if V2)
    pub fn as_v1(&self) -> &SagaContextV1 {
        match self {
            SagaContextEither::V1(ctx) => ctx,
            SagaContextEither::V2(_) => panic!("Not a V1 context"),
        }
    }

    /// Get reference to V2 context (panics if V1)
    pub fn as_v2(&self) -> &SagaContextV2<DefaultSagaMetadata> {
        match self {
            SagaContextEither::V2(ctx) => ctx,
            SagaContextEither::V1(_) => panic!("Not a V2 context"),
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creates_v1_when_flag_false() {
        let saga_id = SagaId::new();
        let result =
            SagaContextFactory::create_context(&saga_id, SagaType::Provisioning, None, None, false);

        assert!(!result.is_v2());
        assert_eq!(result.saga_id(), &saga_id);
    }

    #[test]
    fn test_factory_creates_v2_when_flag_true() {
        let saga_id = SagaId::new();
        let result =
            SagaContextFactory::create_context(&saga_id, SagaType::Provisioning, None, None, true);

        assert!(result.is_v2());
        assert_eq!(result.saga_id(), &saga_id);
    }

    #[test]
    fn test_context_either_preserves_saga_type() {
        let saga_id = SagaId::new();

        let v1_result =
            SagaContextFactory::create_context(&saga_id, SagaType::Execution, None, None, false);
        assert_eq!(v1_result.saga_type(), SagaType::Execution);

        let v2_result =
            SagaContextFactory::create_context(&saga_id, SagaType::Recovery, None, None, true);
        assert_eq!(v2_result.saga_type(), SagaType::Recovery);
    }

    #[test]
    fn test_context_either_to_v1() {
        let saga_id = SagaId::new();

        let v2_result =
            SagaContextFactory::create_context(&saga_id, SagaType::Provisioning, None, None, true);

        let v1_context = v2_result.to_v1();
        assert_eq!(&v1_context.saga_id, &saga_id);
    }
}
