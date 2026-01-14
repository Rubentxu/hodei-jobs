//! Circuit Breaker for Saga Consumers
//!
//! Provides a circuit breaker pattern implementation to prevent cascading failures
//! when saga consumers encounter persistent errors.
//!
//! # States:
//!
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Circuit is tripped, requests fail immediately without calling the consumer
//! - **HalfOpen**: Testing if the consumer has recovered, limited requests allowed

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation, requests pass through
    Closed,
    /// Circuit is tripped, requests fail immediately
    Open,
    /// Testing if the consumer has recovered
    HalfOpen,
}

impl From<u8> for CircuitState {
    fn from(val: u8) -> Self {
        match val {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
}

impl From<CircuitState> for u8 {
    fn from(state: CircuitState) -> Self {
        match state {
            CircuitState::Closed => 0,
            CircuitState::Open => 1,
            CircuitState::HalfOpen => 2,
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u64,
    /// Duration to wait before attempting to close the circuit
    pub open_duration: Duration,
    /// Number of successful calls in half-open state to close the circuit
    pub success_threshold: u64,
    /// Timeout for individual calls
    pub call_timeout: Duration,
    /// Failure rate threshold percentage (0-100) to trigger opening
    pub failure_rate_threshold: u8,
    /// Window size for calculating failure rate
    pub failure_rate_window: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_duration: Duration::from_secs(60),
            success_threshold: 3,
            call_timeout: Duration::from_secs(30),
            failure_rate_threshold: 50,
            failure_rate_window: 100,
        }
    }
}

impl CircuitBreakerConfig {
    /// Creates a configuration optimized for saga consumers.
    ///
    /// Saga consumers are critical path components, so the circuit breaker
    /// is configured to be somewhat sensitive but with quick recovery.
    #[inline]
    pub fn for_saga_consumer() -> Self {
        Self {
            failure_threshold: 5,
            open_duration: Duration::from_secs(30),
            success_threshold: 2,
            call_timeout: Duration::from_secs(60),
            failure_rate_threshold: 50,
            failure_rate_window: 50,
        }
    }

    /// Creates a configuration for high-throughput scenarios.
    #[inline]
    pub fn high_throughput() -> Self {
        Self {
            failure_threshold: 10,
            open_duration: Duration::from_secs(10),
            success_threshold: 5,
            call_timeout: Duration::from_secs(10),
            failure_rate_threshold: 30,
            failure_rate_window: 200,
        }
    }
}

/// Internal circuit breaker state
#[derive(Debug)]
struct CircuitBreakerState {
    /// Current state
    state: AtomicU64,
    /// Number of consecutive failures
    consecutive_failures: AtomicU64,
    /// Number of consecutive successes in half-open state
    consecutive_successes: AtomicU64,
    /// Timestamp when circuit was opened (None if closed)
    opened_at: AtomicU64,
    /// Total number of calls
    total_calls: AtomicU64,
    /// Total number of failures
    total_failures: AtomicU64,
    /// Window of recent call results (1 = success, 0 = failure)
    recent_calls: Vec<AtomicU8>,
    /// Index for circular buffer
    call_index: AtomicUsize,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        let window_size = 100;
        Self {
            state: AtomicU64::new(CircuitState::Closed as u64),
            consecutive_failures: AtomicU64::new(0),
            consecutive_successes: AtomicU64::new(0),
            opened_at: AtomicU64::new(0),
            total_calls: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
            recent_calls: (0..window_size).map(|_| AtomicU8::new(1)).collect(),
            call_index: AtomicUsize::new(0),
        }
    }
}

/// Circuit Breaker implementation
///
/// The circuit breaker prevents cascading failures by failing fast
/// when a consumer is experiencing issues.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Name for identification
    name: String,
    /// Configuration
    config: Arc<CircuitBreakerConfig>,
    /// Internal state
    state: Arc<CircuitBreakerState>,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with the given name and configuration.
    #[inline]
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        Self {
            name,
            config: Arc::new(config),
            state: Arc::new(CircuitBreakerState::default()),
        }
    }

    /// Creates a circuit breaker with default saga consumer configuration.
    #[inline]
    pub fn for_saga_consumer(name: String) -> Self {
        Self::new(name, CircuitBreakerConfig::for_saga_consumer())
    }

    /// Gets the current circuit state.
    #[inline]
    pub fn state(&self) -> CircuitState {
        let current = self.state.state.load(Ordering::SeqCst);
        let state = CircuitState::from(current as u8);

        // Check if we should transition from open to half-open
        if state == CircuitState::Open {
            let opened_at = self.state.opened_at.load(Ordering::SeqCst);
            if opened_at > 0 {
                // Convert millis back to Duration
                let open_duration = Duration::from_millis(opened_at);
                let open_time = std::time::UNIX_EPOCH + open_duration;
                if SystemTime::now()
                    .duration_since(open_time)
                    .unwrap_or(Duration::ZERO)
                    >= self.config.open_duration
                {
                    // Transition to half-open
                    self.state
                        .state
                        .store(CircuitState::HalfOpen as u64, Ordering::SeqCst);
                    self.state.consecutive_successes.store(0, Ordering::SeqCst);
                    return CircuitState::HalfOpen;
                }
            }
        }

        state
    }

    /// Checks if requests should be allowed through.
    #[inline]
    pub fn allow_request(&self) -> bool {
        match self.state() {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => false,
        }
    }

    /// Records a successful call.
    #[inline]
    fn record_success(&self) {
        self.state.total_calls.fetch_add(1, Ordering::SeqCst);

        // Record in circular buffer
        let idx =
            self.state.call_index.fetch_add(1, Ordering::SeqCst) % self.config.failure_rate_window;
        self.state.recent_calls[idx].store(1, Ordering::SeqCst);

        let current = self.state.state.load(Ordering::SeqCst);
        let state = CircuitState::from(current as u8);

        match state {
            CircuitState::HalfOpen => {
                let successes = self
                    .state
                    .consecutive_successes
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                if successes >= self.config.success_threshold {
                    // Close the circuit
                    self.state
                        .state
                        .store(CircuitState::Closed as u64, Ordering::SeqCst);
                    self.state.consecutive_failures.store(0, Ordering::SeqCst);
                    info!(name = %self.name, "Circuit breaker closed (half-open success)");
                }
            }
            CircuitState::Closed => {
                self.state.consecutive_failures.store(0, Ordering::SeqCst);
            }
            CircuitState::Open => {
                // Should not happen, but handle gracefully
            }
        }
    }

    /// Records a failed call.
    #[inline]
    fn record_failure(&self) {
        self.state.total_calls.fetch_add(1, Ordering::SeqCst);
        self.state.total_failures.fetch_add(1, Ordering::SeqCst);

        // Record in circular buffer
        let idx =
            self.state.call_index.fetch_add(1, Ordering::SeqCst) % self.config.failure_rate_window;
        self.state.recent_calls[idx].store(0, Ordering::SeqCst);

        let current = self.state.state.load(Ordering::SeqCst);
        let state = CircuitState::from(current as u8);

        let failures = self
            .state
            .consecutive_failures
            .fetch_add(1, Ordering::SeqCst)
            + 1;

        match state {
            CircuitState::Closed => {
                // Check failure count threshold
                if failures >= self.config.failure_threshold {
                    self.open_circuit();
                }
                // Also check failure rate
                if self.failure_rate() >= self.config.failure_rate_threshold as f64 {
                    self.open_circuit();
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open state opens the circuit again
                self.open_circuit();
            }
            CircuitState::Open => {
                // Already open, nothing to do
            }
        }
    }

    /// Opens the circuit.
    #[inline]
    fn open_circuit(&self) {
        self.state
            .state
            .store(CircuitState::Open as u64, Ordering::SeqCst);
        // Store epoch time in millis
        self.state.opened_at.store(
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_millis() as u64,
            Ordering::SeqCst,
        );
        warn!(
            name = %self.name,
            consecutive_failures = %self.state.consecutive_failures.load(Ordering::SeqCst),
            "Circuit breaker opened"
        );
    }

    /// Calculates the recent failure rate.
    #[inline]
    fn failure_rate(&self) -> f64 {
        let mut failures = 0u64;
        let mut total = 0u64;

        for i in 0..self.config.failure_rate_window {
            let result = self.state.recent_calls[i].load(Ordering::SeqCst);
            total += 1;
            if result == 0 {
                failures += 1;
            }
        }

        if total == 0 {
            0.0
        } else {
            (failures as f64 / total as f64) * 100.0
        }
    }

    /// Gets statistics about the circuit breaker.
    #[inline]
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            name: self.name.clone(),
            state: self.state(),
            total_calls: self.state.total_calls.load(Ordering::SeqCst),
            total_failures: self.state.total_failures.load(Ordering::SeqCst),
            failure_rate: self.failure_rate(),
            consecutive_failures: self.state.consecutive_failures.load(Ordering::SeqCst),
        }
    }

    /// Executes an operation with circuit breaker protection.
    ///
    /// # Arguments
    ///
    /// * `operation` - The async operation to execute
    ///
    /// # Returns
    ///
    /// `Ok(T)` if the operation succeeds and the circuit is closed or half-open
    /// `Err(CircuitBreakerError::Open)` if the circuit is open
    /// `Err(CircuitBreakerError::Timeout)` if the operation times out
    /// `Err(CircuitBreakerError::Failed(e))` if the operation fails
    pub async fn execute<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: Future<Output = Result<T, E>>,
    {
        // Check if circuit allows the request
        if !self.allow_request() {
            return Err(CircuitBreakerError::Open);
        }

        // Execute with timeout
        let result = tokio::time::timeout(self.config.call_timeout, operation).await;

        match result {
            Ok(Ok(value)) => {
                self.record_success();
                Ok(value)
            }
            Ok(Err(e)) => {
                self.record_failure();
                Err(CircuitBreakerError::Failed(e))
            }
            Err(_) => {
                self.record_failure();
                Err(CircuitBreakerError::Timeout)
            }
        }
    }
}

/// Statistics about the circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    /// Name of the circuit breaker
    pub name: String,
    /// Current state
    pub state: CircuitState,
    /// Total number of calls
    pub total_calls: u64,
    /// Total number of failures
    pub total_failures: u64,
    /// Recent failure rate percentage
    pub failure_rate: f64,
    /// Number of consecutive failures
    pub consecutive_failures: u64,
}

/// Error types for circuit breaker operations
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open, requests are being rejected
    #[error("Circuit breaker is open")]
    Open,

    /// Operation timed out
    #[error("Operation timed out")]
    Timeout,

    /// Operation failed with an error
    #[error("Operation failed: {0}")]
    Failed(E),
}

use std::future::Future;

/// Wrapper for saga consumer with circuit breaker protection
#[derive(Debug, Clone)]
pub struct CircuitBreakerSagaConsumer<C> {
    /// Inner consumer
    inner: C,
    /// Circuit breaker
    circuit_breaker: CircuitBreaker,
}

impl<C> CircuitBreakerSagaConsumer<C>
where
    C: Clone,
{
    /// Creates a new circuit breaker wrapped consumer.
    #[inline]
    pub fn new(inner: C, circuit_breaker: CircuitBreaker) -> Self {
        Self {
            inner,
            circuit_breaker,
        }
    }

    /// Gets the inner consumer.
    #[inline]
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// Gets a reference to the circuit breaker.
    #[inline]
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_circuit_breaker_closed_by_default() {
        let breaker = CircuitBreaker::new("test".to_string(), CircuitBreakerConfig::default());
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.allow_request());
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new("test".to_string(), config);

        // Fail several times
        for _ in 0..3 {
            breaker.record_failure();
        }

        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.allow_request());
    }

    #[tokio::test]
    async fn test_circuit_breaker_success_resets_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new("test".to_string(), config);

        // Record some failures
        breaker.record_failure();
        breaker.record_failure();

        // Record success
        breaker.record_success();

        // Fail again (should not open)
        breaker.record_failure();
        breaker.record_failure();

        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            open_duration: Duration::from_millis(50),
            success_threshold: 1,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new("test".to_string(), config);

        // Open the circuit
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for the open duration
        sleep(Duration::from_millis(100)).await;

        // Should transition to half-open
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_on_success_in_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            open_duration: Duration::from_millis(50),
            success_threshold: 2,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new("test".to_string(), config);

        // Open and transition to half-open
        breaker.record_failure();
        sleep(Duration::from_millis(100)).await;
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Record successes
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        breaker.record_success();

        // Should close
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stats() {
        let breaker = CircuitBreaker::new("test".to_string(), CircuitBreakerConfig::default());

        breaker.record_failure();
        breaker.record_failure();
        breaker.record_success();

        let stats = breaker.stats();
        assert_eq!(stats.name, "test");
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.total_calls, 3);
        assert_eq!(stats.total_failures, 2);
    }
}
