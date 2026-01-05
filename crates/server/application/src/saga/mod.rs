//! Saga Orchestration - Application Layer
//!
//! Provides saga-based orchestration for job execution and worker management.
//! This module integrates the domain saga patterns with application services.
//!
//! ## Architecture (EPIC-SAGA-ENGINE-PRODUCTION-READINESS)
//!
//! The saga infrastructure follows the production-ready patterns from the EPIC:
//! - **DynProvisioningSagaCoordinator**: Preferred implementation that injects
//!   services into `SagaContext` for use by saga steps
//! - **CreateInfrastructureStep**: Performs real worker provisioning during saga execution
//! - **InMemorySagaOrchestrator / PostgresSagaOrchestrator**: Handles compensation loops
//!
//! ## Migration from Deprecated Types
//!
//! The following types are **DEPRECATED** and will be removed in v0.32.0:
//! - `ProvisioningSagaCoordinator<T>` - Use `DynProvisioningSagaCoordinator`
//! - `ProvisioningSagaCoordinatorBuilder<T>` - Use `DynProvisioningSagaCoordinatorBuilder`
//!
//! The deprecated types maintain a direct `WorkerProvisioningService` dependency
//! and call `provision_worker()` AFTER saga completion, violating the principle
//! of having saga steps perform real work with compensation support.

pub mod dispatcher_saga;
pub mod provisioning_saga;
pub mod recovery_saga;

pub use dispatcher_saga::{
    DynExecutionSagaDispatcher, DynExecutionSagaDispatcherBuilder,
    DynExecutionSagaDispatcherBuilderError, ExecutionSagaDispatcher, ExecutionSagaDispatcherConfig,
};

pub use provisioning_saga::{
    DynProvisioningSagaCoordinator, DynProvisioningSagaCoordinatorBuilder,
    DynProvisioningSagaCoordinatorBuilderError,
    // DEPRECATED - kept for backward compatibility during migration
    #[deprecated(since = "0.31.0", note = "Use `DynProvisioningSagaCoordinator` instead")]
    ProvisioningSagaCoordinator,
    #[deprecated(since = "0.31.0", note = "Use `DynProvisioningSagaCoordinatorConfig` instead")]
    ProvisioningSagaCoordinatorConfig,
};

pub use recovery_saga::{
    DynRecoverySagaCoordinator, DynRecoverySagaCoordinatorBuilder,
    DynRecoverySagaCoordinatorBuilderError, RecoverySagaCoordinator, RecoverySagaCoordinatorConfig,
};
