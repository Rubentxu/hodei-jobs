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

pub mod dispatcher_saga;
pub mod provisioning_saga;
pub mod recovery_saga;
pub mod timeout_checker;

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
