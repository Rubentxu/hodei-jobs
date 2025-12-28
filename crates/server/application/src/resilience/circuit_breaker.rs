//! Circuit Breaker for Resilience

use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "CLOSED"),
            CircuitState::Open => write!(f, "OPEN"),
            CircuitState::HalfOpen => write!(f, "HALF_OPEN"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub success_threshold: u64,
    pub timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Error)]
#[error("Circuit breaker is {state}, request rejected")]
pub struct CircuitOpenError {
    pub state: CircuitState,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: Mutex<CircuitState>,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_state_change: Mutex<Instant>,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_config(name, CircuitBreakerConfig::default())
    }

    pub fn with_config(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            state: Mutex::new(CircuitState::Closed),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_state_change: Mutex::new(Instant::now()),
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state.lock().unwrap().clone()
    }

    pub async fn execute<F, T>(&self, operation: F) -> Result<T, CircuitOpenError>
    where
        F: Future<Output = Result<T, CircuitOpenError>>,
    {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();

        if *state == CircuitState::Open {
            let last_change = *self.last_state_change.lock().unwrap();
            if now.duration_since(last_change) >= self.config.timeout {
                *state = CircuitState::HalfOpen;
                self.success_count.store(0, Ordering::Relaxed);
                self.failure_count.store(0, Ordering::Relaxed);
                *self.last_state_change.lock().unwrap() = now;
                info!(circuit = self.name, "Circuit transitioned to HALF_OPEN");
            }
        }

        if *state == CircuitState::Open {
            return Err(CircuitOpenError {
                state: state.clone(),
            });
        }

        drop(state);
        match operation.await {
            Ok(result) => {
                let mut state = self.state.lock().unwrap();
                self.success_count.fetch_add(1, Ordering::Relaxed);
                self.failure_count.store(0, Ordering::Relaxed);
                if *state == CircuitState::HalfOpen {
                    if self.success_count.load(Ordering::Relaxed) >= self.config.success_threshold {
                        *state = CircuitState::Closed;
                        *self.last_state_change.lock().unwrap() = Instant::now();
                        info!(circuit = self.name, "Circuit closed");
                    }
                }
                Ok(result)
            }
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                self.failure_count.fetch_add(1, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
                let failure_count = self.failure_count.load(Ordering::Relaxed);
                if *state == CircuitState::HalfOpen {
                    *state = CircuitState::Open;
                    *self.last_state_change.lock().unwrap() = Instant::now();
                    error!(circuit = self.name, failure_count, "Circuit reopened");
                } else if *state == CircuitState::Closed
                    && failure_count >= self.config.failure_threshold
                {
                    *state = CircuitState::Open;
                    *self.last_state_change.lock().unwrap() = Instant::now();
                    error!(circuit = self.name, failure_count, "Circuit opened");
                }
                Err(e)
            }
        }
    }

    pub async fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        *self.last_state_change.lock().unwrap() = Instant::now();
        info!(circuit = self.name, "Circuit reset");
    }

    pub async fn force_open(&self) {
        let mut state = self.state.lock().unwrap();
        *state = CircuitState::Open;
        *self.last_state_change.lock().unwrap() = Instant::now();
        warn!(circuit = self.name, "Circuit force-opened");
    }

    pub fn metrics(&self) -> CircuitBreakerMetrics {
        CircuitBreakerMetrics {
            name: self.name.clone(),
            state: self.state.lock().unwrap().clone(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    pub name: String,
    pub state: CircuitState,
    pub failure_count: u64,
    pub success_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    async fn success_op() -> Result<&'static str, CircuitOpenError> {
        Ok("success")
    }

    async fn fail_op() -> Result<&'static str, CircuitOpenError> {
        Err(CircuitOpenError {
            state: CircuitState::Open,
        })
    }

    #[tokio::test]
    async fn test_circuit_stays_closed() {
        let circuit = CircuitBreaker::new("test");
        let result = circuit.execute(success_op()).await;
        assert!(result.is_ok());
        assert_eq!(circuit.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let circuit = CircuitBreaker::with_config("test", config);
        for _ in 0..3 {
            let _ = circuit.execute(fail_op()).await;
        }
        assert_eq!(circuit.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_rejects_when_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let circuit = CircuitBreaker::with_config("test", config);
        let _ = circuit.execute(fail_op()).await;
        assert!(circuit.execute(success_op()).await.is_err());
    }

    #[tokio::test]
    async fn test_circuit_half_open_timeout_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let circuit = CircuitBreaker::with_config("test", config);
        let _ = circuit.execute(fail_op()).await;
        assert_eq!(circuit.state(), CircuitState::Open);

        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = circuit.execute(success_op()).await;
        assert!(result.is_ok());
        assert_eq!(circuit.state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let circuit = CircuitBreaker::with_config("test", config);
        let _ = circuit.execute(fail_op()).await;

        tokio::time::sleep(Duration::from_millis(50)).await;

        let _ = circuit.execute(success_op()).await;
        assert_eq!(circuit.state(), CircuitState::HalfOpen);

        let _ = circuit.execute(success_op()).await;
        assert_eq!(circuit.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let circuit = CircuitBreaker::with_config("test", config);
        let _ = circuit.execute(fail_op()).await;
        circuit.reset().await;
        assert_eq!(circuit.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_force_open() {
        let circuit = CircuitBreaker::new("test");
        circuit.force_open().await;
        assert_eq!(circuit.state(), CircuitState::Open);
    }
}
