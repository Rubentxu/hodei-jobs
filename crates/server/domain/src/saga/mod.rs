//! Saga Pattern Framework
//!
//! This module provides a complete implementation of the Saga pattern
//! for orchestrating complex, multi-step workflows with automatic
//! compensation (rollback) capabilities.

pub mod orchestrator;
pub mod provisioning;
pub mod repository;
pub mod types;

// Re-exports from types module
pub use types::{
    Saga, SagaContext, SagaError, SagaExecutionResult, SagaId, SagaOrchestrator, SagaResult,
    SagaState, SagaStep, SagaType,
};

// Re-exports from repository module
pub use repository::{SagaRepository, SagaStepData, SagaStepId, SagaStepState};

// Re-exports from orchestrator module
pub use orchestrator::{InMemorySagaOrchestrator, SagaOrchestratorConfig};

// Re-exports from provisioning module
pub use provisioning::{
    CreateInfrastructureStep, ProvisioningSaga, PublishProvisionedEventStep, RegisterWorkerStep,
    ValidateProviderCapacityStep,
};
