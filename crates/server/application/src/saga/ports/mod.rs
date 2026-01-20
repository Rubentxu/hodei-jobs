//!
//! # Saga Ports
//!
//! Adapter ports for saga infrastructure.
//!

pub mod circuit_breaker;
pub mod migration_checklist;
pub mod rate_limiter;
pub mod stuck_detection;

pub use circuit_breaker::{
    CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerEvent, CircuitBreakerState,
    InMemoryCircuitBreaker, SagaCircuitBreaker,
};
pub use migration_checklist::{
    ChecklistCategory, ChecklistItem, ChecklistReport, ChecklistStatus, ChecklistValidationResult,
    MigrationChecklistService, WorkflowChecklist,
};
pub use rate_limiter::{
    InMemoryRateLimiter, RateLimitConfig, RateLimitError, RateLimitPermit, SagaRateLimiter,
};
pub use stuck_detection::{
    InMemoryStuckDetector, SagaStatusInfo, SagaStuckDetector, StuckDetectionConfig,
    StuckDetectionError, StuckDetectionResult, StuckDetectionTask,
};
