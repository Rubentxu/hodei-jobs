//!
//! # Saga Ports for Legacy Infrastructure
//!
//! Adapter ports that allow saga-engine v4.0 to integrate with legacy
//! infrastructure components.
//!

pub mod circuit_breaker;
pub mod dual_write_monitor;
pub mod migration_checklist;
pub mod migration_helper;
pub mod rate_limiter;
pub mod stuck_detection;

pub use circuit_breaker::{
    CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerEvent, CircuitBreakerState,
    InMemoryCircuitBreaker, SagaCircuitBreaker,
};
pub use dual_write_monitor::{
    ConsistencyMetrics, ConsistencyMonitorTask, ConsistencyReport, DualWriteConfig, DualWriteError,
    DualWriteMonitor, InMemoryMetricsRecorder, Inconsistency, InconsistencyType,
    WorkflowStateComparator,
};
pub use migration_checklist::{
    ChecklistCategory, ChecklistItem, ChecklistReport, ChecklistStatus, ChecklistValidationResult,
    MigrationChecklistService, WorkflowChecklist,
};
pub use migration_helper::{
    InMemoryMigrationHelper, MigrationDecision, MigrationHelperConfig, MigrationService,
    MigrationStrategy, MigrationSummary, SagaMigrationHelper, SagaTypeInfo, SagaTypeRegistry,
};
pub use rate_limiter::{
    InMemoryRateLimiter, RateLimitConfig, RateLimitError, RateLimitPermit, SagaRateLimiter,
};
pub use stuck_detection::{
    InMemoryStuckDetector, SagaStatusInfo, SagaStuckDetector, StuckDetectionConfig,
    StuckDetectionError, StuckDetectionResult, StuckDetectionTask,
};
