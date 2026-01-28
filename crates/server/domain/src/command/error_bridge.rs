// ============================================================================
// Error Bridge - Saga Engine V4.1 Integration
// ============================================================================
//!
//! Provides bridge functionality between domain errors and Saga Engine error
//! handling. This module enables automatic error classification, retry decisions,
//! and error statistics tracking for saga-based workflows.
//!
//! ## Architecture
//!
//! ```text
//! +------------------+     +------------------+     +-------------------+
//! |  Domain Errors   | --> |  Error Bridge    | --> | Saga Engine Core  |
//! |  (CommandError,  |     |  - Classification|     |  - ExecutionError |
//! |   ProviderError) |     |  - Statistics    |     |  - ErrorDecision  |
//! +------------------+     |  - Conversion    |     +-------------------+
//!                          +------------------+               |
//!                                                              v
//!                          +------------------+     +-------------------+
//!                          |  Command Bus     | <-- |  Saga Executor    |
//!                          |  Integration     |     |  - Retry logic    |
//!                          +------------------+     |  - Compensation   |
//!                                                    +-------------------+
//! ```
//!
//! ## Usage
//!
//! The ErrorBridge provides automatic error classification for saga workflows:
//!
//! ```ignore
//! let bridge = ErrorBridge::new();
//! // Errors are automatically classified and decisions made
//! ```

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use saga_engine_core::error::{
    ClassifiedError, ErrorBehaviorWithConfig, ErrorCategory, ErrorDecision, ErrorStats,
    ExecutionError, ExecutionResult,
};

use crate::command::error::CommandError;
use crate::providers::errors::{ProviderConnectionError, ProviderInitializationError};
use crate::saga::errors::SagaCoreError;

/// Result type for bridge operations
pub type BridgeResult<T> = Result<T, BridgeError>;

/// Errors that can occur in error bridge operations
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Classification failed: {message}")]
    ClassificationFailed { message: String },

    #[error("Stats operation failed: {message}")]
    StatsError { message: String },
}

/// Configuration for error bridge behavior
#[derive(Debug, Clone)]
pub struct ErrorBridgeConfig {
    /// Enable automatic retry for transient errors
    pub enable_transient_retry: bool,

    /// Enable automatic stats collection
    pub enable_stats: bool,

    /// Maximum retries before giving up
    pub max_retries: u8,

    /// Default delay for retries (ms)
    pub default_retry_delay_ms: u64,

    /// Enable fallback to compensation on repeated failures
    pub fallback_to_compensation: bool,
}

impl Default for ErrorBridgeConfig {
    fn default() -> Self {
        Self {
            enable_transient_retry: true,
            enable_stats: true,
            max_retries: 3,
            default_retry_delay_ms: 1000,
            fallback_to_compensation: true,
        }
    }
}

/// Central error bridge for saga integration
///
/// Provides unified error handling across all domain error types with:
/// - Automatic classification using ClassifiedError trait
/// - Statistics collection for monitoring
/// - Decision making based on error characteristics
/// - Integration with saga executor
#[derive(Debug, Clone)]
pub struct ErrorBridge {
    config: ErrorBridgeConfig,
    stats: Arc<Mutex<ErrorStats>>,
}

impl ErrorBridge {
    /// Create a new error bridge with default configuration
    pub fn new() -> Self {
        Self {
            config: ErrorBridgeConfig::default(),
            stats: Arc::new(Mutex::new(ErrorStats::default())),
        }
    }

    /// Create a new error bridge with custom configuration
    pub fn with_config(config: ErrorBridgeConfig) -> Self {
        Self {
            config,
            stats: Arc::new(Mutex::new(ErrorStats::default())),
        }
    }

    /// Get shared access to error statistics
    pub fn stats(&self) -> Arc<Mutex<ErrorStats>> {
        Arc::clone(&self.stats)
    }

    /// Get statistics snapshot
    pub fn stats_snapshot(&self) -> ErrorStats {
        self.stats.lock().unwrap().clone()
    }

    /// Classify an error and get the decision for saga handling
    ///
    /// This is the primary entry point for saga error handling.
    /// It classifies the error, records statistics, and determines
    /// the appropriate saga action.
    pub fn classify_and_decide<E: ClassifiedError>(&self, error: &E) -> ErrorDecision {
        // Record the error for statistics
        if self.config.enable_stats {
            self.stats.lock().unwrap().record_error(error.category());
        }

        // Get behavior and determine decision directly without cloning
        let behavior = error.behavior();
        let decision = match behavior {
            ErrorBehaviorWithConfig::Retry { config } if config.max_attempts > 0 => {
                ErrorDecision::Retry
            }
            ErrorBehaviorWithConfig::Retry { .. } => ErrorDecision::Fail, // max_attempts == 0
            ErrorBehaviorWithConfig::RetryOnce { .. } => ErrorDecision::Retry,
            ErrorBehaviorWithConfig::SkipToCompensation => ErrorDecision::Compensate,
            ErrorBehaviorWithConfig::Fail => ErrorDecision::Fail,
            ErrorBehaviorWithConfig::ManualIntervention => ErrorDecision::Pause,
        };

        // Record the decision
        if self.config.enable_stats {
            self.stats.lock().unwrap().record_decision(decision);
        }

        decision
    }

    /// Convert a domain error to ExecutionResult
    ///
    /// Wraps a domain error in ExecutionError with classification metadata.
    /// This is useful for passing errors from domain handlers to saga executor.
    pub fn to_execution_result<E: ClassifiedError + Clone>(
        &self,
        error: E,
    ) -> ExecutionResult<(), E> {
        Err(ExecutionError::new(error))
    }

    /// Convert a Result to ExecutionResult with bridge classification
    ///
    /// Transforms a standard Result into ExecutionResult, preserving
    /// the error type while adding saga-relevant metadata.
    pub fn bridge_result<T, E: ClassifiedError + Clone>(
        &self,
        result: Result<T, E>,
    ) -> ExecutionResult<T, E> {
        result.map_err(|error| {
            if self.config.enable_stats {
                self.stats.lock().unwrap().record_error(error.category());
            }
            ExecutionError::new(error)
        })
    }

    /// Get retry behavior for a classified error
    pub fn retry_behavior<E: ClassifiedError>(&self, error: &E) -> ErrorBehaviorWithConfig {
        // Check for custom retry config first
        if let Some(config) = error.retry_config() {
            return ErrorBehaviorWithConfig::Retry { config };
        }
        if let Some(config) = error.retry_once_config() {
            return ErrorBehaviorWithConfig::RetryOnce { config };
        }
        error.behavior()
    }

    /// Calculate next retry delay based on attempt count
    pub fn next_retry_delay<E: ClassifiedError>(&self, error: &E, attempt: u8) -> Option<Duration> {
        self.retry_behavior(error).calculate_delay(attempt)
    }

    /// Check if an error should be retried
    pub fn should_retry<E: ClassifiedError>(&self, error: &E, attempts: u8) -> bool {
        let behavior = self.retry_behavior(error);
        attempts < behavior.max_attempts() && behavior.is_retry()
    }

    /// Get the error category for logging/monitoring
    pub fn error_category<E: ClassifiedError>(&self, error: &E) -> ErrorCategory {
        error.category()
    }

    /// Check if error is retryable (infrastructure category)
    pub fn is_retryable<E: ClassifiedError>(&self, error: &E) -> bool {
        error.is_retryable()
    }

    /// Get configuration for this bridge
    pub fn config(&self) -> &ErrorBridgeConfig {
        &self.config
    }

    /// Update bridge configuration at runtime
    pub fn update_config(&mut self, config: ErrorBridgeConfig) {
        self.config = config;
    }

    /// Reset statistics counters
    pub fn reset_stats(&mut self) {
        self.stats = Arc::new(Mutex::new(ErrorStats::default()));
    }
}

impl Default for ErrorBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Domain-Specific Bridge Helpers
// ============================================================================

impl ErrorBridge {
    /// Bridge a CommandError to saga decision
    pub fn classify_command_error(&self, error: &CommandError) -> ErrorDecision {
        self.classify_and_decide(error)
    }

    /// Bridge a ProviderConnectionError to saga decision
    pub fn classify_provider_error(&self, error: &ProviderConnectionError) -> ErrorDecision {
        self.classify_and_decide(error)
    }

    /// Bridge a ProviderInitializationError to saga decision
    pub fn classify_initialization_error(
        &self,
        error: &ProviderInitializationError,
    ) -> ErrorDecision {
        self.classify_and_decide(error)
    }

    /// Bridge a SagaCoreError to saga decision
    pub fn classify_saga_error(&self, error: &SagaCoreError) -> ErrorDecision {
        self.classify_and_decide(error)
    }

    /// Convert CommandError to ExecutionResult for saga executor
    pub fn command_to_execution(&self, error: CommandError) -> ExecutionResult<(), CommandError> {
        self.to_execution_result(error)
    }

    /// Convert ProviderConnectionError to ExecutionResult for saga executor
    pub fn provider_to_execution(
        &self,
        error: ProviderConnectionError,
    ) -> ExecutionResult<(), ProviderConnectionError> {
        self.to_execution_result(error)
    }
}

// ============================================================================
// Metrics and Monitoring
// ============================================================================

impl ErrorBridge {
    /// Get retry success rate from statistics
    pub fn retry_success_rate(&self) -> f64 {
        self.stats.lock().unwrap().retry_success_rate()
    }

    /// Get failure rate from statistics
    pub fn failure_rate(&self) -> f64 {
        self.stats.lock().unwrap().failure_rate()
    }

    /// Get total error count
    pub fn total_errors(&self) -> u64 {
        self.stats.lock().unwrap().total_errors
    }

    /// Get compensation count
    pub fn compensation_count(&self) -> u64 {
        self.stats.lock().unwrap().compensation_count
    }

    /// Get failure count
    pub fn failure_count(&self) -> u64 {
        self.stats.lock().unwrap().failure_count
    }
}

// ============================================================================
// From implementations for common error types
// ============================================================================

impl From<CommandError> for ExecutionError<CommandError> {
    fn from(error: CommandError) -> Self {
        ExecutionError::new(error)
    }
}

impl From<ProviderConnectionError> for ExecutionError<ProviderConnectionError> {
    fn from(error: ProviderConnectionError) -> Self {
        ExecutionError::new(error)
    }
}

impl From<ProviderInitializationError> for ExecutionError<ProviderInitializationError> {
    fn from(error: ProviderInitializationError) -> Self {
        ExecutionError::new(error)
    }
}

impl From<SagaCoreError> for ExecutionError<SagaCoreError> {
    fn from(error: SagaCoreError) -> Self {
        ExecutionError::new(error)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::error::CommandError;
    use crate::providers::errors::{ProviderConnectionError, ProviderInitializationError};
    use crate::saga::SagaId;
    use crate::saga::errors::SagaCoreError;

    #[test]
    fn test_command_error_classification() {
        let bridge = ErrorBridge::new();

        // Timeout should be retryable
        let timeout_error = CommandError::Timeout {
            duration: Duration::from_secs(30),
        };
        let decision = bridge.classify_command_error(&timeout_error);
        assert_eq!(decision, ErrorDecision::Retry);

        // Validation failed should fail (nothing to compensate)
        let validation_error = CommandError::ValidationFailed {
            message: "Invalid input".to_string(),
        };
        let decision = bridge.classify_and_decide(&validation_error);
        assert_eq!(decision, ErrorDecision::Fail);
    }

    #[test]
    fn test_provider_error_classification() {
        let bridge = ErrorBridge::new();

        // Network error should be retryable
        let network_error = ProviderConnectionError::Network("connection refused".to_string());
        let decision = bridge.classify_provider_error(&network_error);
        assert_eq!(decision, ErrorDecision::Retry);

        // Configuration error should fail
        let config_error = ProviderConnectionError::Configuration("missing field".to_string());
        let decision = bridge.classify_provider_error(&config_error);
        assert_eq!(decision, ErrorDecision::Fail);
    }

    #[test]
    fn test_saga_error_classification() {
        let bridge = ErrorBridge::new();

        // Timeout should be retryable
        let timeout_error = SagaCoreError::Timeout {
            saga_id: SagaId::new(),
            duration: Duration::from_secs(60),
        };
        let decision = bridge.classify_saga_error(&timeout_error);
        assert_eq!(decision, ErrorDecision::Retry);

        // Not found should trigger compensation
        let not_found_error = SagaCoreError::NotFound {
            saga_id: SagaId::new(),
        };
        let decision = bridge.classify_saga_error(&not_found_error);
        assert_eq!(decision, ErrorDecision::Compensate);
    }

    #[test]
    fn test_error_stats() {
        let mut bridge = ErrorBridge::new();

        let timeout_error = CommandError::Timeout {
            duration: Duration::from_secs(30),
        };
        bridge.classify_and_decide(&timeout_error);

        let validation_error = CommandError::ValidationFailed {
            message: "Invalid input".to_string(),
        };
        bridge.classify_and_decide(&validation_error);

        assert_eq!(bridge.total_errors(), 2);
        assert!(bridge.retry_success_rate() >= 0.0);
    }

    #[test]
    fn test_retry_behavior() {
        let bridge = ErrorBridge::new();

        let timeout_error = CommandError::Timeout {
            duration: Duration::from_secs(30),
        };

        let behavior = bridge.retry_behavior(&timeout_error);
        assert!(behavior.is_retry());
        assert_eq!(behavior.max_attempts(), 1); // RetryOnce
    }

    #[test]
    fn test_should_retry() {
        let bridge = ErrorBridge::new();

        let timeout_error = CommandError::Timeout {
            duration: Duration::from_secs(30),
        };

        assert!(bridge.should_retry(&timeout_error, 0));
        assert!(!bridge.should_retry(&timeout_error, 1)); // Only 1 attempt allowed
    }

    #[test]
    fn test_error_bridge_config() {
        let config = ErrorBridgeConfig {
            enable_transient_retry: false,
            enable_stats: false,
            max_retries: 5,
            default_retry_delay_ms: 2000,
            fallback_to_compensation: false,
        };

        let bridge = ErrorBridge::with_config(config);
        assert_eq!(bridge.config.max_retries, 5);
        assert!(!bridge.config.enable_transient_retry);
    }
}
