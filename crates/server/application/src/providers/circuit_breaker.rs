//! Provider Circuit Breaker
//!
//! Implements the Circuit Breaker pattern to prevent cascade failures
//! when providers become unavailable. Transitions through states:
//! - Closed: Normal operation, requests allowed
//! - Open: Failing, requests rejected immediately
//! - Half-Open: Testing recovery, limited requests allowed

use hodei_server_domain::shared_kernel::ProviderId;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Circuit breaker state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation, requests allowed
    Closed,
    /// Failing, requests rejected immediately
    Open,
    /// Testing recovery, limited requests allowed
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u64,
    /// Number of successes in half-open to close circuit
    pub success_threshold: u64,
    /// Time in open state before attempting half-open
    pub timeout_duration: Duration,
    /// Time between success checks in half-open state
    pub reset_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_duration: Duration::from_secs(60),
            reset_timeout: Duration::from_secs(5),
        }
    }
}

/// Circuit breaker event for observability
#[derive(Debug, Clone)]
pub enum CircuitBreakerEvent {
    /// Circuit transitioned to a new state
    StateTransition {
        from: CircuitState,
        to: CircuitState,
        provider_id: ProviderId,
    },
    /// A request was allowed through
    RequestAllowed {
        provider_id: ProviderId,
        state: CircuitState,
    },
    /// A request was rejected
    RequestRejected {
        provider_id: ProviderId,
        state: CircuitState,
    },
    /// A failure was recorded
    FailureRecorded {
        provider_id: ProviderId,
        failure_count: u64,
    },
    /// A success was recorded
    SuccessRecorded {
        provider_id: ProviderId,
        success_count: u64,
    },
}

/// Provider Circuit Breaker
///
/// Prevents cascade failures by detecting repeated failures and temporarily
/// stopping requests to a failing provider.
#[derive(Clone)]
pub struct ProviderCircuitBreaker {
    /// Provider ID this circuit breaker is for
    provider_id: ProviderId,
    /// Current state (protected by mutex for async safety)
    state: Arc<Mutex<CircuitState>>,
    /// Number of consecutive failures
    failure_count: Arc<AtomicU64>,
    /// Number of consecutive successes (half-open)
    success_count: Arc<AtomicU64>,
    /// Timestamp of last failure
    last_failure: Arc<Mutex<Option<Instant>>>,
    /// Timestamp of last success
    last_success: Arc<Mutex<Option<Instant>>>,
    /// Configuration
    config: CircuitBreakerConfig,
    /// Event sender for observability
    event_sender: Arc<Mutex<Option<tokio::sync::mpsc::Sender<CircuitBreakerEvent>>>>,
}

impl ProviderCircuitBreaker {
    /// Create a new circuit breaker with default configuration
    pub fn new(provider_id: ProviderId) -> Self {
        Self::with_config(provider_id, CircuitBreakerConfig::default())
    }

    /// Create a circuit breaker with custom configuration
    pub fn with_config(provider_id: ProviderId, config: CircuitBreakerConfig) -> Self {
        Self {
            provider_id,
            state: Arc::new(Mutex::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            last_failure: Arc::new(Mutex::new(None)),
            last_success: Arc::new(Mutex::new(None)),
            config,
            event_sender: Arc::new(Mutex::new(None)),
        }
    }

    /// Set an event sender for observability
    pub async fn set_event_sender(&self, sender: tokio::sync::mpsc::Sender<CircuitBreakerEvent>) {
        let mut sender_ref = self.event_sender.lock().await;
        *sender_ref = Some(sender);
    }

    /// Emit an event if a sender is configured
    async fn emit_event(&self, event: CircuitBreakerEvent) {
        let sender_opt = self.event_sender.lock().await;
        if let Some(sender) = sender_opt.as_ref() {
            if let Err(e) = sender.send(event).await {
                warn!("Failed to emit circuit breaker event: {}", e);
            }
        }
    }

    /// Check if a request can proceed
    ///
    /// Returns `true` if the request should be allowed,
    /// `false` if the circuit is open and request should be rejected.
    pub async fn can_proceed(&self) -> bool {
        let state = self.state.lock().await.clone();

        let can_proceed = match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                let last_failure = self.last_failure.lock().await.clone();
                if let Some(last) = last_failure {
                    if last.elapsed() >= self.config.timeout_duration {
                        // Transition to half-open
                        self.transition_to(CircuitState::HalfOpen).await;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        };

        let current_state = self.state.lock().await.clone();
        if can_proceed {
            self.emit_event(CircuitBreakerEvent::RequestAllowed {
                provider_id: self.provider_id.clone(),
                state: current_state,
            })
            .await;
        } else {
            self.emit_event(CircuitBreakerEvent::RequestRejected {
                provider_id: self.provider_id.clone(),
                state: current_state,
            })
            .await;
        }

        can_proceed
    }

    /// Record a successful operation
    pub async fn record_success(&self) {
        let successes = self.success_count.fetch_add(1, Ordering::SeqCst);
        let state = self.state.lock().await.clone();

        *self.last_success.lock().await = Some(Instant::now());

        self.emit_event(CircuitBreakerEvent::SuccessRecorded {
            provider_id: self.provider_id.clone(),
            success_count: successes + 1,
        })
        .await;

        if state == CircuitState::HalfOpen && (successes + 1) >= self.config.success_threshold {
            // Transition back to closed
            self.failure_count.store(0, Ordering::SeqCst);
            self.success_count.store(0, Ordering::SeqCst);
            *self.state.lock().await = CircuitState::Closed;

            self.transition_to(CircuitState::Closed).await;

            info!(
                provider_id = %self.provider_id,
                "Circuit breaker closed - provider recovered"
            );
        }
    }

    /// Record a failed operation
    pub async fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::SeqCst);
        let state = self.state.lock().await.clone();

        *self.last_failure.lock().await = Some(Instant::now());

        self.emit_event(CircuitBreakerEvent::FailureRecorded {
            provider_id: self.provider_id.clone(),
            failure_count: failures + 1,
        })
        .await;

        if state == CircuitState::Closed && (failures + 1) >= self.config.failure_threshold {
            // Open the circuit
            *self.state.lock().await = CircuitState::Open;

            self.transition_to(CircuitState::Open).await;

            error!(
                provider_id = %self.provider_id,
                failure_count = failures + 1,
                "Circuit breaker opened - too many failures"
            );
        } else if state == CircuitState::HalfOpen {
            // Any failure in half-open goes back to open
            *self.state.lock().await = CircuitState::Open;
            self.success_count.store(0, Ordering::SeqCst);

            self.transition_to(CircuitState::Open).await;

            error!(
                provider_id = %self.provider_id,
                "Circuit breaker opened - recovery test failed"
            );
        }
    }

    /// Get current state
    pub async fn state(&self) -> CircuitState {
        self.state.lock().await.clone()
    }

    /// Get current failure count
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::SeqCst)
    }

    /// Get time since last failure
    pub async fn time_since_last_failure(&self) -> Option<Duration> {
        let last = self.last_failure.lock().await.clone();
        last.map(|i| i.elapsed())
    }

    /// Reset the circuit breaker to closed state
    pub async fn reset(&self) {
        let old_state = self.state.lock().await.clone();
        *self.state.lock().await = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        *self.last_failure.lock().await = None;
        *self.last_success.lock().await = None;

        if old_state != CircuitState::Closed {
            self.emit_event(CircuitBreakerEvent::StateTransition {
                from: old_state,
                to: CircuitState::Closed,
                provider_id: self.provider_id.clone(),
            })
            .await;
        }

        info!(
            provider_id = %self.provider_id,
            "Circuit breaker reset to closed state"
        );
    }

    /// Transition to a new state and emit event
    async fn transition_to(&self, new_state: CircuitState) {
        let old_state = self.state.lock().await.clone();
        let new_state_cloned = new_state.clone();

        if old_state != new_state {
            *self.state.lock().await = new_state;

            let debug_old_state = old_state.clone();
            let debug_new_state = new_state_cloned.clone();

            self.emit_event(CircuitBreakerEvent::StateTransition {
                from: old_state,
                to: new_state_cloned,
                provider_id: self.provider_id.clone(),
            })
            .await;

            debug!(
                provider_id = %self.provider_id,
                from = ?debug_old_state,
                to = ?debug_new_state,
                "Circuit breaker state transition"
            );
        }
    }
}

/// Manager for multiple circuit breakers
#[derive(Clone)]
pub struct CircuitBreakerManager {
    /// Map of provider ID to circuit breaker
    breakers: Arc<Mutex<HashMap<ProviderId, ProviderCircuitBreaker>>>,
    /// Default configuration
    default_config: CircuitBreakerConfig,
}

impl CircuitBreakerManager {
    /// Create a new manager with default configuration
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Create a manager with custom default configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: Arc::new(Mutex::new(HashMap::new())),
            default_config: config,
        }
    }

    /// Get or create a circuit breaker for a provider
    pub async fn get_or_create(&self, provider_id: &ProviderId) -> ProviderCircuitBreaker {
        let mut breakers = self.breakers.lock().await;

        if let Some(breaker) = breakers.get(provider_id) {
            return breaker.clone();
        }

        let breaker =
            ProviderCircuitBreaker::with_config(provider_id.clone(), self.default_config.clone());
        breakers.insert(provider_id.clone(), breaker.clone());

        breaker
    }

    /// Get a circuit breaker if it exists
    pub async fn get(&self, provider_id: &ProviderId) -> Option<ProviderCircuitBreaker> {
        self.breakers.lock().await.get(provider_id).cloned()
    }

    /// Remove a circuit breaker
    pub async fn remove(&self, provider_id: &ProviderId) {
        self.breakers.lock().await.remove(provider_id);
    }

    /// Reset all circuit breakers
    pub async fn reset_all(&self) {
        let breakers = self.breakers.lock().await;
        for breaker in breakers.values() {
            breaker.reset().await;
        }
    }

    /// Get health status of all circuit breakers
    pub async fn health_status(&self) -> Vec<(ProviderId, CircuitState, u64)> {
        let breakers = self.breakers.lock().await;
        let mut results = Vec::with_capacity(breakers.len());
        for (id, breaker) in breakers.iter() {
            results.push((id.clone(), breaker.state().await, breaker.failure_count()));
        }
        results
    }
}

impl Default for CircuitBreakerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::shared_kernel::ProviderId;

    use std::time::Instant;

    fn create_test_breaker() -> (ProviderId, ProviderCircuitBreaker) {
        let provider_id = ProviderId::new();
        let breaker = ProviderCircuitBreaker::new(provider_id.clone());
        (provider_id, breaker)
    }

    #[tokio::test]
    async fn test_closed_state_allows_requests() {
        let (_, breaker) = create_test_breaker();
        assert!(breaker.can_proceed().await);
    }

    #[tokio::test]
    async fn test_opens_after_failure_threshold() {
        let (_, breaker) = create_test_breaker();

        // Record failures up to threshold
        for _ in 0..5 {
            breaker.record_failure().await;
        }

        assert_eq!(breaker.state().await, CircuitState::Open);
        assert!(!breaker.can_proceed().await);
    }

    #[tokio::test]
    async fn test_half_open_after_timeout() {
        let (_, breaker) = create_test_breaker();

        // Open the circuit
        for _ in 0..5 {
            breaker.record_failure().await;
        }
        assert_eq!(breaker.state().await, CircuitState::Open);

        // Simulate timeout by setting last_failure to 60+ seconds ago
        *breaker.last_failure.lock().await = Some(Instant::now() - Duration::from_secs(61));

        // Should allow request now (triggers half-open)
        assert!(breaker.can_proceed().await);
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_closes_after_successes_in_half_open() {
        let (_, breaker) = create_test_breaker();

        // Open and transition to half-open
        for _ in 0..5 {
            breaker.record_failure().await;
        }
        // Simulate timeout by setting last_failure to 60+ seconds ago
        *breaker.last_failure.lock().await = Some(Instant::now() - Duration::from_secs(61));
        breaker.can_proceed().await;

        // Record successes
        for _ in 0..3 {
            breaker.record_success().await;
        }

        assert_eq!(breaker.state().await, CircuitState::Closed);
        assert!(breaker.can_proceed().await);
    }

    #[tokio::test]
    async fn test_reopens_on_failure_in_half_open() {
        let (_, breaker) = create_test_breaker();

        // Open and transition to half-open
        for _ in 0..5 {
            breaker.record_failure().await;
        }
        // Simulate timeout by setting last_failure to 60+ seconds ago
        *breaker.last_failure.lock().await = Some(Instant::now() - Duration::from_secs(61));
        breaker.can_proceed().await;

        // Record one failure
        breaker.record_failure().await;

        assert_eq!(breaker.state().await, CircuitState::Open);
        assert!(!breaker.can_proceed().await);
    }

    #[tokio::test]
    async fn test_reset() {
        let (_, breaker) = create_test_breaker();

        // Open the circuit
        for _ in 0..5 {
            breaker.record_failure().await;
        }

        // Reset
        breaker.reset().await;

        assert_eq!(breaker.state().await, CircuitState::Closed);
        assert!(breaker.can_proceed().await);
    }

    #[tokio::test]
    async fn test_failure_counting() {
        let (_, breaker) = create_test_breaker();

        assert_eq!(breaker.failure_count(), 0);

        breaker.record_failure().await;
        assert_eq!(breaker.failure_count(), 1);

        breaker.record_failure().await;
        assert_eq!(breaker.failure_count(), 2);
    }
}
