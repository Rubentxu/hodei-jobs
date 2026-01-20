//!
//! # Circuit Breaker Port
//!
//! Provides a circuit breaker pattern implementation for saga step protection.
//!

use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Failing, reject all requests
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerEvent {
    Success,
    Failure,
    Timeout,
}

/// Configuration for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub success_threshold: u64,
    pub timeout: std::time::Duration,
    pub name: String,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: std::time::Duration::from_secs(60),
            name: "default".to_string(),
        }
    }
}

/// Circuit breaker error
#[derive(Debug, Error, Clone)]
#[error("Circuit breaker {name} is open")]
pub struct CircuitBreakerError {
    pub name: String,
    pub state: CircuitBreakerState,
    pub opened_at: chrono::DateTime<chrono::Utc>,
}

impl CircuitBreakerError {
    pub fn new(
        name: String,
        state: CircuitBreakerState,
        opened_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            name,
            state,
            opened_at,
        }
    }
}

/// Circuit breaker port for saga step protection
#[async_trait]
pub trait SagaCircuitBreaker: Send + Sync {
    /// Checks if the operation is allowed
    async fn can_execute(&self, key: &str) -> Result<(), CircuitBreakerError>;
    /// Records the result of an operation
    async fn record_result(&self, key: &str, event: CircuitBreakerEvent);
    /// Gets the current state
    async fn state(&self, key: &str) -> CircuitBreakerState;
    /// Resets the circuit breaker
    async fn reset(&self, key: &str);
}

/// In-memory circuit breaker implementation
#[derive(Debug)]
pub struct InMemoryCircuitBreaker {
    states: Arc<RwLock<HashMap<String, CircuitBreakerEntry>>>,
    default_config: CircuitBreakerConfig,
}

#[derive(Debug)]
struct CircuitBreakerEntry {
    state: CircuitBreakerState,
    failure_count: u64,
    success_count: u64,
    last_failure: Option<chrono::DateTime<chrono::Utc>>,
    last_success: Option<chrono::DateTime<chrono::Utc>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreakerEntry {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure: None,
            last_success: None,
            config,
        }
    }

    pub fn can_execute(&self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::HalfOpen => true,
            CircuitBreakerState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = self.last_failure {
                    let elapsed = chrono::Utc::now() - last_failure;
                    elapsed > chrono::Duration::from_std(self.config.timeout).unwrap()
                } else {
                    false
                }
            }
        }
    }

    pub fn record_event(&mut self, event: CircuitBreakerEvent) {
        let now = chrono::Utc::now();
        match event {
            CircuitBreakerEvent::Success => {
                self.success_count += 1;
                self.last_success = Some(now);

                if self.state == CircuitBreakerState::HalfOpen {
                    if self.success_count >= self.config.success_threshold {
                        self.state = CircuitBreakerState::Closed;
                        self.failure_count = 0;
                    }
                } else {
                    // In closed state, reset failure count on success
                    self.failure_count = 0;
                }
            }
            CircuitBreakerEvent::Failure | CircuitBreakerEvent::Timeout => {
                self.failure_count += 1;
                self.last_failure = Some(now);

                if self.state == CircuitBreakerState::HalfOpen {
                    // Immediate back to open
                    self.state = CircuitBreakerState::Open;
                } else if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                }
            }
        }
    }
}

impl InMemoryCircuitBreaker {
    pub fn new(default_config: CircuitBreakerConfig) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            default_config,
        }
    }

    async fn get_or_create(
        &self,
        key: &str,
    ) -> tokio::sync::RwLockWriteGuard<'_, HashMap<String, CircuitBreakerEntry>> {
        let mut states = self.states.write().await;
        if !states.contains_key(key) {
            states.insert(
                key.to_string(),
                CircuitBreakerEntry::new(self.default_config.clone()),
            );
        }
        states
    }
}

#[async_trait]
impl SagaCircuitBreaker for InMemoryCircuitBreaker {
    async fn can_execute(&self, key: &str) -> Result<(), CircuitBreakerError> {
        let states = self.states.read().await;
        if let Some(entry) = states.get(key) {
            if entry.can_execute() {
                Ok(())
            } else {
                Err(CircuitBreakerError::new(
                    key.to_string(),
                    entry.state,
                    entry.last_failure.unwrap_or_else(chrono::Utc::now),
                ))
            }
        } else {
            // Non-existent key is allowed (will be created on first record_result)
            Ok(())
        }
    }

    async fn record_result(&self, key: &str, event: CircuitBreakerEvent) {
        let mut states = self.states.write().await;
        let entry = states
            .entry(key.to_string())
            .or_insert_with(|| CircuitBreakerEntry::new(self.default_config.clone()));
        entry.record_event(event);
    }

    async fn state(&self, key: &str) -> CircuitBreakerState {
        let states = self.states.read().await;
        states
            .get(key)
            .map(|e| e.state)
            .unwrap_or(CircuitBreakerState::Closed)
    }

    async fn reset(&self, key: &str) {
        let mut states = self.states.write().await;
        if let Some(entry) = states.get_mut(key) {
            *entry = CircuitBreakerEntry::new(entry.config.clone());
        }
    }
}

/// Saga step with circuit breaker protection
pub struct CircuitBreakerSagaStep<S> {
    inner: S,
    circuit_breaker: Arc<dyn SagaCircuitBreaker>,
    step_key: String,
}

impl<S> CircuitBreakerSagaStep<S> {
    pub fn new(inner: S, circuit_breaker: Arc<dyn SagaCircuitBreaker>, step_key: String) -> Self {
        Self {
            inner,
            circuit_breaker,
            step_key,
        }
    }
}

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_allows_requests() {
        let breaker = Arc::new(InMemoryCircuitBreaker::new(CircuitBreakerConfig::default()));
        let key = "test-step";

        assert!(breaker.can_execute(key).await.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let breaker = Arc::new(InMemoryCircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: std::time::Duration::from_secs(60),
            name: "test".to_string(),
        }));
        let key = "test-step-2";

        // Record 3 failures
        for _ in 0..3 {
            breaker
                .record_result(key, CircuitBreakerEvent::Failure)
                .await;
        }

        // Now should be open
        let state = breaker.state(key).await;
        assert_eq!(state, CircuitBreakerState::Open);

        // Should be denied
        assert!(breaker.can_execute(key).await.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_resets() {
        let breaker = Arc::new(InMemoryCircuitBreaker::new(CircuitBreakerConfig::default()));
        let key = "test-step-3";

        // Open the circuit
        for _ in 0..5 {
            breaker
                .record_result(key, CircuitBreakerEvent::Failure)
                .await;
        }

        // Reset
        breaker.reset(key).await;

        // Should be closed again
        let state = breaker.state(key).await;
        assert_eq!(state, CircuitBreakerState::Closed);
    }
}
