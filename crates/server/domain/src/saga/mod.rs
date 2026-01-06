//! Saga Pattern Framework
//!
//! This module provides a complete implementation of the Saga pattern
//! for orchestrating complex, multi-step workflows with automatic
//! compensation (rollback) capabilities.
//!
//! # Saga Types
//!
//! - **ProvisioningSaga**: Worker creation and registration
//! - **ExecutionSaga**: Job dispatch and execution
//! - **RecoverySaga**: Worker failure recovery and job reassignment
//! - **CancellationSaga**: Job cancellation with worker cleanup
//! - **TimeoutSaga**: Job timeout handling
//! - **CleanupSaga**: Orphaned resource cleanup

pub mod cancellation;
pub mod cleanup;
pub mod engine_config;
pub mod execution;
pub mod metrics;
pub mod orchestrator;
pub mod provisioning;
pub mod recovery;
pub mod repository;
pub mod stuck_detector;
pub mod timeout;
pub mod types;

// Re-exports from types module
pub use types::{
    Saga, SagaContext, SagaError, SagaExecutionResult, SagaId, SagaOrchestrator, SagaResult,
    SagaServices, SagaState, SagaStep, SagaType,
};

// Re-exports from repository module
pub use repository::{SagaRepository, SagaStepData, SagaStepId, SagaStepState};

// Re-exports from metrics module
pub use metrics::{NoOpSagaMetrics, SagaMetrics};

// Re-exports from engine_config module
pub use engine_config::{SagaEngineConfig, SagaFeature};

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

// Re-exports from cancellation module
pub use cancellation::{
    CancellationSaga, NotifyWorkerStep, ReleaseWorkerStep, UpdateJobStateStep,
    ValidateCancellationStep,
};

// Re-exports from timeout module
pub use timeout::{
    CleanupWorkerStep, MarkJobFailedStep, TerminateWorkerStep as TimeoutTerminateWorkerStep,
    TimeoutSaga, ValidateTimeoutStep,
};

// Re-exports from cleanup module
pub use cleanup::{
    CleanupSaga, IdentifyOrphanedJobsStep, IdentifyUnhealthyWorkersStep, PublishCleanupMetricsStep,
    ResetOrphanedJobsStep,
};

// Re-exports from stuck_detector module (EPIC-46 GAP-12)
pub use stuck_detector::{
    InMemoryStuckSagaDetector, StuckSagaDetector, StuckSagaDetectorConfig, StuckSagaDetectorError,
    StuckSagaInfo,
};

// Idempotency helpers
pub use types::{saga_id_for_job, saga_id_for_provisioning, saga_id_for_recovery};
