//! Resilience Module
//!
//! Provides resilience patterns like Circuit Breaker for protecting
//! dispatch operations from cascading failures.

pub mod circuit_breaker;

pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics, CircuitOpenError, CircuitState,
};
