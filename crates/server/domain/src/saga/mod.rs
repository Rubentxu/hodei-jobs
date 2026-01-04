//! Saga Pattern Framework
//!
//! This module provides a complete implementation of the Saga pattern
//! for orchestrating complex, multi-step workflows with automatic
//! compensation (rollback) capabilities.

pub mod execution;
pub mod orchestrator;
pub mod provisioning;
pub mod recovery;
pub mod repository;
pub mod types;

// Re-exports from types module
pub use types::{
    Saga, SagaContext, SagaError, SagaExecutionResult, SagaId, SagaOrchestrator, SagaResult,
    SagaServices, SagaState, SagaStep, SagaType,
};

// Re-exports from repository module
pub use repository::{SagaRepository, SagaStepData, SagaStepId, SagaStepState};

// Re-exports from orchestrator module
pub use orchestrator::{InMemorySagaOrchestrator, OrchestratorError, SagaOrchestratorConfig};

// Re-exports from provisioning module
pub use provisioning::{
    CreateInfrastructureStep, ProvisioningSaga, PublishProvisionedEventStep, RegisterWorkerStep,
    ValidateProviderCapacityStep,
};

// Re-exports from execution module
pub use execution::{
    AssignWorkerStep, CompleteJobStep, ExecuteJobStep, ExecutionSaga, ValidateJobStep,
};

// Re-exports from recovery module
pub use recovery::{
    CancelOldWorkerStep, CheckWorkerConnectivityStep, ProvisionNewWorkerStep, RecoverySaga,
    TerminateOldWorkerStep, TransferJobStep,
};

// Idempotency helpers
pub use types::{saga_id_for_job, saga_id_for_provisioning, saga_id_for_recovery};
