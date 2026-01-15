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

pub mod audit_trail;
pub mod cancellation;
pub mod circuit_breaker;
pub mod cleanup;
pub mod commands;
pub mod engine_config;
pub mod errors;
pub mod event_handlers;
pub mod event_sourcing;
pub mod execution;
pub mod metrics;
pub mod orchestrator;
pub mod provisioning;
pub mod recovery;
pub mod repository;
pub mod retry_policy; // EPIC-46 GAP-25: RetryPolicy with exponential backoff
pub mod stuck_detector;
pub mod timeout;
pub mod timeout_config; // EPIC-85: Configurable saga timeouts
pub mod types; // EPIC-85 US-05: Circuit Breaker for Saga Consumers

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

// Re-exports from audit_trail module (EPIC-85 US-15)
pub use audit_trail::{
    SagaAuditConsumer, SagaAuditEntry, SagaAuditEventType, SagaAuditTrail, TracingAuditConsumer,
    VecAuditConsumer,
};

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

// Commands re-exports
pub use commands::{
    CreateWorkerCommand, CreateWorkerError, CreateWorkerHandler, DestroyWorkerCommand,
    DestroyWorkerError, DestroyWorkerHandler,
};

// Re-exports from timeout_config module
pub use timeout_config::{SagaTimeoutConfig, TimeoutAware};

// Re-exports from circuit_breaker module
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerSagaConsumer,
    CircuitBreakerStats, CircuitState,
};

// Re-exports from event_sourcing module
pub use event_sourcing::{
    EventSourcedSagaState, EventSourcingBuilder, EventSourcingLayer, InMemorySagaEventStore,
    SAGA_EVENT_VERSION, SagaEvent, SagaEventId, SagaEventStore, SagaEventStoreError, SagaEventType,
};
