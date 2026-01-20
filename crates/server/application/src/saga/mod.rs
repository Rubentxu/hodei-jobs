//! Saga Orchestration - Application Layer
//!
//! Provides saga-based orchestration for job execution and worker management.
//! This module integrates domain saga patterns with saga-engine v4.0.
//!
//! ## Architecture
//!
//! The saga infrastructure uses saga-engine v4.0 for durable execution:
//! - **SagaPort<W>**: Type-safe port abstraction for workflow execution
//! - **Workflow Definitions**: Recovery, Timeout, Cancellation, Cleanup, Execution, Provisioning
//! - **Durable Execution**: Powered by saga-engine v4.0 with EventStore and TaskQueue
//!
//! ## Workflow Implementations
//!
//! Provides workflow definitions implementing WorkflowDefinition trait:
//! - Recovery: Worker failure recovery workflow
//! - Timeout: Job timeout handling workflow
//! - Cancellation: Graceful cancellation workflow
//! - Cleanup: Audit log cleanup workflow
//! - Execution: Job execution workflow
//! - Provisioning: Worker provisioning workflow

pub mod workflows;

pub mod port;

pub mod adapters;

pub mod bridge;

pub mod ports;

pub mod dispatcher_saga;
pub mod provisioning_saga;
pub mod recovery_saga;
pub mod timeout_checker;

pub mod sync_executor;

pub use port::types::{SagaExecutionId, SagaPortConfig, SagaPortResult, WorkflowState};

pub use dispatcher_saga::{DynExecutionSagaDispatcher, ExecutionSagaDispatcherConfig};
pub use provisioning_saga::{
    DynProvisioningSagaCoordinator, ProvisioningSagaCoordinatorConfig, ProvisioningSagaError,
};
pub use recovery_saga::{
    DynRecoverySagaCoordinator, RecoverySagaCoordinator, RecoverySagaCoordinatorConfig,
    RecoverySagaError,
};
pub use timeout_checker::{
    TimeoutCheckResult, TimeoutChecker, TimeoutCheckerConfig, TimeoutCheckerError,
};
