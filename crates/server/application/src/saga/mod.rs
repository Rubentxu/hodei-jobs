//! Saga Orchestration - Application Layer
//!
//! Provides saga-based orchestration for job execution and worker management.
//! This module integrates domain saga patterns with application services.
//!
//! ## Architecture
//!
//! The saga infrastructure follows production-ready patterns:
//! - **DynProvisioningSagaCoordinator**: Preferred implementation that injects
//!   services into `SagaContext` for use by saga steps
//! - **CreateInfrastructureStep**: Performs real worker provisioning during saga execution
//! - **PostgresSagaOrchestrator**: Handles compensation loops
//!
//! ## EPIC-94: Saga Engine v4.0 Migration
//!
//! This module provides an adapter layer for migrating to saga-engine v4.0:
//! - [`port`]: SagaPort trait abstraction for saga execution
//! - [`adapters`]: SagaEngineV4Adapter implementation
//!
//! ## Workflow Implementations (EPIC-94)
//!
//! Provides workflow definitions for saga-engine v4.0 migration,
//! implementing WorkflowDefinition trait for each saga type.
//!
//! ## Port Abstraction (EPIC-94)
//!
//! Port abstraction layer for saga execution.
//!
//! ## Adapter Layer (EPIC-94)
//!
//! Adapter layer for saga migration.
//!
//! ## Bridge Layer (EPIC-94)
//!
//! Bridge layer for CommandBus to Activity migration.
//!
//! ## Feature Flags (EPIC-94)
//!
//! Feature flags for controlling saga migration.
//!
//! ## Compatibility Test (EPIC-94)
//!
//! Test suite for verifying saga compatibility.

pub mod workflows;

pub mod port;

pub mod adapters;

pub mod bridge;

pub mod ports;

pub mod feature_flags;

pub mod compatibility_test;

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
    DynRecoverySagaCoordinator, RecoverySagaCoordinatorConfig, RecoverySagaError,
};
pub use timeout_checker::{
    DynTimeoutChecker, DynTimeoutCheckerBuilder, TimeoutCheckResult, TimeoutChecker,
    TimeoutCheckerConfig, TimeoutCheckerDynInterface,
};
