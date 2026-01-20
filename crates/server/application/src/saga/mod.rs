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
//! The `DynProvisioningSagaCoordinator` is the canonical implementation that:
//! - Injects services (`WorkerProvisioning`, `WorkerRegistry`, `EventBus`) into `SagaContext`
//! - Lets saga steps (`CreateInfrastructureStep`, `RegisterWorkerStep`) perform real work
//! - Provides automatic compensation via saga steps on failure
//!
//! ## EPIC-94: Saga Engine v4.0 Migration
//!
//! This module also provides the adapter layer for migrating to saga-engine v4.0:
//! - [`port`]: SagaPort trait abstraction for saga execution
//! - [`adapters`]: LegacySagaAdapter and SagaEngineV4Adapter implementations
//! - [`adapters::factory`]: Factory for creating adapters

pub mod dispatcher_saga;
pub mod provisioning_saga;
pub mod recovery_saga;
pub mod timeout_checker;

/// Workflow implementations for saga-engine v4.0 (EPIC-94)
pub mod workflows;

/// Port abstraction layer for saga execution (EPIC-94)
pub mod port;

/// Adapter layer for saga migration (EPIC-94)
pub mod adapters;

/// Bridge layer for CommandBus to Activity migration (EPIC-94)
pub mod bridge;

/// Legacy migration support for saga-engine v4.0 (EPIC-94)
pub mod legacy;

/// Adapter ports for legacy infrastructure integration (EPIC-94)
pub mod ports;
pub use dispatcher_saga::{
    DynExecutionSagaDispatcher, DynExecutionSagaDispatcherBuilder,
    DynExecutionSagaDispatcherBuilderError, ExecutionSagaDispatcher, ExecutionSagaDispatcherConfig,
};

pub use provisioning_saga::{
    DynProvisioningSagaCoordinator, DynProvisioningSagaCoordinatorBuilder,
    DynProvisioningSagaCoordinatorBuilderError, ProvisioningSagaCoordinatorConfig,
};

pub use recovery_saga::{
    DynRecoverySagaCoordinator, DynRecoverySagaCoordinatorBuilder,
    DynRecoverySagaCoordinatorBuilderError, RecoverySagaCoordinator, RecoverySagaCoordinatorConfig,
};

pub use timeout_checker::{
    TimeoutCheckResult, TimeoutChecker, TimeoutCheckerBuilder, TimeoutCheckerConfig,
};
