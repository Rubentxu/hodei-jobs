//!
//! # Saga Ports for Legacy Infrastructure
//!
//! Adapter ports that allow saga-engine v4.0 to integrate with legacy
//! infrastructure components.
//!

pub mod circuit_breaker;
pub mod rate_limiter;
pub mod stuck_detection;

pub use circuit_breaker::{
    CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerEvent, CircuitBreakerState,
    InMemoryCircuitBreaker, SagaCircuitBreaker,
};
pub use rate_limiter::{
    InMemoryRateLimiter, RateLimitConfig, RateLimitError, RateLimitPermit, SagaRateLimiter,
};
pub use stuck_detection::{
    InMemoryStuckDetector, SagaStatusInfo, SagaStuckDetector, StuckDetectionConfig,
    StuckDetectionError, StuckDetectionResult, StuckDetectionTask,
};
