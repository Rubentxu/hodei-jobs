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
pub mod commands; // Commands for Command Bus pattern
pub mod engine_config;
pub mod errors; // EPIC-46 GAP-21: Strongly typed error enums
pub mod event_handlers; // EPIC-46 GAP-10/11: Reactive event handlers
pub mod execution;
pub mod metrics;
pub mod orchestrator;
pub mod provisioning;
pub mod recovery;
pub mod repository;
pub mod retry_policy; // EPIC-46 GAP-25: RetryPolicy with exponential backoff
pub mod stuck_detector;
pub mod timeout;
pub mod types;

// Re-exports from types module
pub use types::{
    Saga, SagaContext, SagaError, SagaExecutionResult, SagaId, SagaOrchestrator, SagaResult,
    SagaServices, SagaState, SagaStep, SagaType,
};

// Re-exports from errors module (EPIC-46 GAP-21)
pub use errors::{
    CancellationSagaError, CleanupSagaError, ExecutionSagaError, IntoSagaCoreError,
    ProvisioningSagaError, RecoverySagaError, SagaCoreError, TimeoutSagaError,
};

// Re-exports from event_handlers module (EPIC-46 GAP-10/11)
pub use event_handlers::{
    HandlerResult, LoggingHandler, MetricsRecordingHandler, NotificationType, SagaAction,
    SagaEventHandler, SagaEventHandlerBuilder, SagaEventHandlerRegistry, SagaLifecycleEvent,
};

// Re-exports from retry_policy module (EPIC-46 GAP-25)
pub use retry_policy::{
    RetryOutcome, RetryPolicy, RetryPolicyBuilder, RetryResult, RetryableOperation,
    execute_with_retry,
};

// Re-exports from repository module
pub use repository::{SagaRepository, SagaStepData, SagaStepId, SagaStepState};

// Re-exports from metrics module
pub use metrics::{InMemorySagaMetrics, NoOpSagaMetrics, SagaConcurrencyMetrics, SagaMetrics};

// Re-exports from engine_config module
pub use engine_config::{SagaEngineConfig, SagaFeature};

// Re-exports from orchestrator module
pub use orchestrator::{InMemorySagaOrchestrator, OrchestratorError, SagaOrchestratorConfig};

// Re-exports from provisioning module
pub use provisioning::{
    CreateInfrastructureStep, ProvisioningSaga, PublishProvisionedEventStep,
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
    MarkJobFailedStep, ReleaseWorkerStep as TimeoutReleaseWorkerStep, TerminateWorkerStep,
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

// Commands re-exports (Épica 52: Saga Refactoring to Use Command Bus)
pub use commands::{
    CreateWorkerCommand, CreateWorkerError, CreateWorkerHandler, DestroyWorkerCommand,
    DestroyWorkerError, DestroyWorkerHandler,
};
