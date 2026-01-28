//!
//! # Error Classification
//!
//! High-performance error classification for saga engine.
//! Distinguishes between retryable infrastructure errors and non-retryable domain errors.
//!
//! ## Performance Considerations
//!
//! - Uses `u8` for category and behavior representation (zero-cost abstraction)
//! - No heap allocations in hot paths
//! - Copy semantics for small types
//! - Const-evaluable category comparisons
//!

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Error category for saga decision making
///
/// Using `u8` representation for zero-cost enum operations.
/// Category 0 is reserved for performance optimizations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum ErrorCategory {
    /// Infrastructure error (network, timeout, DB connection)
    /// Saga engine SHOULD retry automatically
    Infrastructure = 1,

    /// Validation error (invalid input, corrupted data)
    /// Saga engine SHOULD NOT retry (would fail again)
    Validation = 2,

    /// Domain error (business rule violated)
    /// Saga engine SHOULD compensate (not retry)
    Domain = 3,

    /// Transient domain error (may succeed after delay)
    /// Saga engine MAY retry after delay
    TransientDomain = 4,

    /// Fatal error (bugs, invalid states)
    /// Saga engine SHOULD fail completely
    Fatal = 5,
}

/// Error behavior for saga engine
///
/// Optimized for pattern matching without heap allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ErrorBehavior {
    /// Retry with exponential backoff
    Retry = 1,

    /// Retry once after a delay
    RetryOnce = 2,

    /// Skip to compensation (no retry)
    SkipToCompensation = 3,

    /// Fail immediately (no retry, no compensation)
    Fail = 4,

    /// Pause for manual intervention
    ManualIntervention = 5,
}

/// Retry configuration embedded in behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u8,

    /// Base delay in milliseconds (will be multiplied by 2^attempt)
    pub base_delay_ms: u64,
}

impl Default for RetryConfig {
    /// Default: 3 attempts with 1 second base delay
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
        }
    }
}

/// Once retry configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryOnceConfig {
    /// Delay in milliseconds before retry
    pub delay_ms: u64,
}

impl Default for RetryOnceConfig {
    fn default() -> Self {
        Self { delay_ms: 5000 }
    }
}

/// Behavior with embedded retry configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorBehaviorWithConfig {
    /// Retry with exponential backoff
    Retry { config: RetryConfig },

    /// Retry once after delay
    RetryOnce { config: RetryOnceConfig },

    /// Skip to compensation
    SkipToCompensation,

    /// Fail immediately
    Fail,

    /// Manual intervention required
    ManualIntervention,
}

impl From<ErrorBehavior> for ErrorBehaviorWithConfig {
    fn from(behavior: ErrorBehavior) -> Self {
        match behavior {
            ErrorBehavior::Retry => ErrorBehaviorWithConfig::Retry {
                config: RetryConfig::default(),
            },
            ErrorBehavior::RetryOnce => ErrorBehaviorWithConfig::RetryOnce {
                config: RetryOnceConfig::default(),
            },
            ErrorBehavior::SkipToCompensation => ErrorBehaviorWithConfig::SkipToCompensation,
            ErrorBehavior::Fail => ErrorBehaviorWithConfig::Fail,
            ErrorBehavior::ManualIntervention => ErrorBehaviorWithConfig::ManualIntervention,
        }
    }
}

impl ErrorBehaviorWithConfig {
    /// Check if this behavior involves retry
    #[inline]
    pub fn is_retry(&self) -> bool {
        matches!(
            self,
            ErrorBehaviorWithConfig::Retry { .. } | ErrorBehaviorWithConfig::RetryOnce { .. }
        )
    }

    /// Calculate delay for this attempt (0-indexed)
    #[inline]
    pub fn calculate_delay(&self, attempt: u8) -> Option<Duration> {
        match self {
            ErrorBehaviorWithConfig::Retry { config } => {
                if attempt >= config.max_attempts {
                    return None;
                }
                // Exponential backoff: base * 2^attempt
                let delay_ms = config.base_delay_ms * (2_u64.pow(attempt as u32));
                Some(Duration::from_millis(delay_ms))
            }
            ErrorBehaviorWithConfig::RetryOnce { config } => {
                if attempt == 0 {
                    Some(Duration::from_millis(config.delay_ms))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Get max attempts for retry behaviors
    #[inline]
    pub fn max_attempts(&self) -> u8 {
        match self {
            ErrorBehaviorWithConfig::Retry { config } => config.max_attempts,
            ErrorBehaviorWithConfig::RetryOnce { .. } => 1,
            _ => 0,
        }
    }
}

/// Trait for classifying errors and determining saga behavior
///
/// This trait allows any error type to participate in saga error handling.
/// Implementors should categorize their errors correctly for proper saga behavior.
///
/// ## Performance
///
/// - Uses `'static` bound to enable static dispatch
/// - No heap allocations in trait method calls
/// - Copy return types to avoid cloning
pub trait ClassifiedError: Send + Sync + 'static {
    /// Get the error category
    #[inline]
    fn category(&self) -> ErrorCategory {
        ErrorCategory::Fatal
    }

    /// Get the error behavior (defaults to category behavior)
    #[inline]
    fn behavior(&self) -> ErrorBehaviorWithConfig {
        default_behavior(self.category())
    }

    /// Check if error is retryable (infrastructure errors)
    #[inline]
    fn is_retryable(&self) -> bool {
        self.category() == ErrorCategory::Infrastructure
    }

    /// Optional: custom retry config for this specific error
    ///
    /// Returns None to use default behavior
    #[inline]
    fn retry_config(&self) -> Option<RetryConfig> {
        None
    }

    /// Optional: custom retry-once config for this specific error
    #[inline]
    fn retry_once_config(&self) -> Option<RetryOnceConfig> {
        None
    }
}

/// Get default behavior for a category (const-evaluable)
#[inline]
pub const fn default_behavior(category: ErrorCategory) -> ErrorBehaviorWithConfig {
    match category {
        ErrorCategory::Infrastructure => ErrorBehaviorWithConfig::Retry {
            config: RetryConfig {
                max_attempts: 3,
                base_delay_ms: 1000,
            },
        },
        ErrorCategory::Validation => ErrorBehaviorWithConfig::Fail,
        ErrorCategory::Domain => ErrorBehaviorWithConfig::SkipToCompensation,
        ErrorCategory::TransientDomain => ErrorBehaviorWithConfig::RetryOnce {
            config: RetryOnceConfig { delay_ms: 5000 },
        },
        ErrorCategory::Fatal => ErrorBehaviorWithConfig::Fail,
    }
}

/// Execution error wrapper with classification
///
/// Wraps any error and provides saga-relevant information.
/// Designed for zero-overhead wrapping of domain errors.
#[derive(Debug, Clone)]
pub struct ExecutionError<E: ClassifiedError> {
    /// The underlying error
    pub error: E,

    /// Number of retry attempts made
    pub attempts: u8,

    /// Whether this is a retry vs original attempt
    pub is_retry: bool,

    /// Timestamp of the error
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl<E: ClassifiedError> ExecutionError<E> {
    /// Create a new execution error
    #[inline]
    pub fn new(error: E) -> Self {
        Self {
            error,
            attempts: 0,
            is_retry: false,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create with attempt count
    #[inline]
    pub fn with_attempts(mut self, attempts: u8, is_retry: bool) -> Self {
        self.attempts = attempts;
        self.is_retry = is_retry;
        self
    }

    /// Get the error category
    #[inline]
    pub fn category(&self) -> ErrorCategory {
        self.error.category()
    }

    /// Get the error behavior
    #[inline]
    pub fn behavior(&self) -> ErrorBehaviorWithConfig {
        // Use custom config if available
        if let Some(config) = self.error.retry_config() {
            return ErrorBehaviorWithConfig::Retry { config };
        }
        if let Some(config) = self.error.retry_once_config() {
            return ErrorBehaviorWithConfig::RetryOnce { config };
        }
        self.error.behavior()
    }

    /// Check if should retry
    #[inline]
    pub fn should_retry(&self) -> bool {
        let behavior = self.behavior();
        let max = behavior.max_attempts();
        self.attempts < max && behavior.is_retry()
    }

    /// Check if the underlying error is retryable
    #[inline]
    pub fn is_retryable(&self) -> bool {
        self.error.is_retryable()
    }

    /// Get next delay if retrying
    #[inline]
    pub fn next_delay(&self) -> Option<Duration> {
        self.behavior().calculate_delay(self.attempts)
    }

    /// Convert to a different error type
    #[inline]
    pub fn map_err<F, T: ClassifiedError>(self, f: impl FnOnce(E) -> T) -> ExecutionError<T> {
        ExecutionError {
            error: f(self.error),
            attempts: self.attempts,
            is_retry: self.is_retry,
            timestamp: self.timestamp,
        }
    }
}

impl<E: ClassifiedError + std::fmt::Debug> std::fmt::Display for ExecutionError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ExecutionError({:?}, attempts={})",
            self.error, self.attempts
        )
    }
}

impl<E: ClassifiedError + std::error::Error> std::error::Error for ExecutionError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

/// Result type for activity execution with classification
pub type ExecutionResult<T, E> = std::result::Result<T, ExecutionError<E>>;

/// Helper to convert from standard error result
/// Note: Due to Rust orphan rules, we cannot implement From<Result<T, E>>.
/// Use `.map_err(ExecutionError::new)` instead.
///
/// ```rust
/// use saga_engine_core::error::classification::{ExecutionError, ExecutionResult, ClassifiedError};
///
/// fn example<E: ClassifiedError>(result: Result<String, E>) -> ExecutionResult<String, E> {
///     result.map_err(ExecutionError::new)
/// }
/// ```

/// Decision made by saga engine after an error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDecision {
    /// Retry the activity
    Retry,

    /// Skip to compensation
    Compensate,

    /// Fail the saga
    Fail,

    /// Pause for manual intervention
    Pause,
}

impl ErrorDecision {
    /// Determine decision from error and attempt
    #[inline]
    pub fn from_error<E: ClassifiedError>(error: &ExecutionError<E>) -> Self {
        let behavior = error.behavior();

        // Check if we should retry
        if error.should_retry() {
            return ErrorDecision::Retry;
        }

        // Determine final action from behavior
        match behavior {
            ErrorBehaviorWithConfig::Retry { .. } => ErrorDecision::Fail,
            ErrorBehaviorWithConfig::RetryOnce { .. } => ErrorDecision::Compensate,
            ErrorBehaviorWithConfig::SkipToCompensation => ErrorDecision::Compensate,
            ErrorBehaviorWithConfig::Fail => ErrorDecision::Fail,
            ErrorBehaviorWithConfig::ManualIntervention => ErrorDecision::Pause,
        }
    }
}

/// Statistics for error handling performance
#[derive(Debug, Default, Clone)]
pub struct ErrorStats {
    /// Total errors encountered
    pub total_errors: u64,

    /// Errors by category
    pub by_category: [u64; 6], // Index by ErrorCategory as u8 - 1

    /// Total retries performed
    pub total_retries: u64,

    /// Successful retries (error resolved after retry)
    pub successful_retries: u64,

    /// Errors that led to compensation
    pub compensation_count: u64,

    /// Errors that led to saga failure
    pub failure_count: u64,

    /// Errors that required manual intervention
    pub manual_intervention_count: u64,
}

impl ErrorStats {
    /// Record an error
    #[inline]
    pub fn record_error(&mut self, category: ErrorCategory) {
        self.total_errors += 1;
        let idx = category as u8 as usize;
        if idx > 0 && idx < self.by_category.len() {
            self.by_category[idx] += 1;
        }
    }

    /// Record a retry
    #[inline]
    pub fn record_retry(&mut self, successful: bool) {
        self.total_retries += 1;
        if successful {
            self.successful_retries += 1;
        }
    }

    /// Record a decision
    #[inline]
    pub fn record_decision(&mut self, decision: ErrorDecision) {
        match decision {
            ErrorDecision::Compensate => self.compensation_count += 1,
            ErrorDecision::Fail => self.failure_count += 1,
            ErrorDecision::Pause => self.manual_intervention_count += 1,
            ErrorDecision::Retry => {}
        }
    }

    /// Get retry success rate
    #[inline]
    pub fn retry_success_rate(&self) -> f64 {
        if self.total_retries == 0 {
            1.0
        } else {
            self.successful_retries as f64 / self.total_retries as f64
        }
    }

    /// Get failure rate
    #[inline]
    pub fn failure_rate(&self) -> f64 {
        if self.total_errors == 0 {
            0.0
        } else {
            self.failure_count as f64 / self.total_errors as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    pub enum TestError {
        #[error("Infrastructure error")]
        Infrastructure,

        #[error("Domain error: {reason}")]
        Domain { reason: String },

        #[error("Validation error: {field}")]
        Validation { field: String },
    }

    impl ClassifiedError for TestError {
        fn category(&self) -> ErrorCategory {
            match self {
                TestError::Infrastructure => ErrorCategory::Infrastructure,
                TestError::Domain { .. } => ErrorCategory::Domain,
                TestError::Validation { .. } => ErrorCategory::Validation,
            }
        }
    }

    #[test]
    fn test_error_category_ordering() {
        assert!((ErrorCategory::Infrastructure as u8) < (ErrorCategory::Fatal as u8));
        assert!((ErrorCategory::Validation as u8) < (ErrorCategory::Domain as u8));
    }

    #[test]
    fn test_behavior_delay_calculation() {
        let behavior = ErrorBehaviorWithConfig::Retry {
            config: RetryConfig {
                max_attempts: 3,
                base_delay_ms: 1000,
            },
        };

        assert_eq!(
            behavior.calculate_delay(0),
            Some(Duration::from_millis(1000))
        );
        assert_eq!(
            behavior.calculate_delay(1),
            Some(Duration::from_millis(2000))
        );
        assert_eq!(
            behavior.calculate_delay(2),
            Some(Duration::from_millis(4000))
        );
        assert_eq!(behavior.calculate_delay(3), None); // Exceeded max
    }

    #[test]
    fn test_execution_error_retry() {
        let error = TestError::Infrastructure;
        let exec_error = ExecutionError::new(error);

        assert!(exec_error.is_retryable());
        assert!(exec_error.should_retry());
        assert_eq!(exec_error.attempts, 0);

        let exec_error = exec_error.with_attempts(2, true);
        assert_eq!(exec_error.attempts, 2);
        assert!(exec_error.is_retry);

        // After 3 attempts, should not retry
        let exec_error = exec_error.with_attempts(3, true);
        assert!(!exec_error.should_retry());
    }

    #[test]
    fn test_decision_from_error() {
        let infra_error = ExecutionError::new(TestError::Infrastructure);
        let domain_error = ExecutionError::new(TestError::Domain {
            reason: "insufficient funds".to_string(),
        });

        assert_eq!(
            ErrorDecision::from_error(&infra_error),
            ErrorDecision::Retry
        );

        assert_eq!(
            ErrorDecision::from_error(&domain_error),
            ErrorDecision::Compensate
        );
    }

    #[test]
    fn test_stats() {
        let mut stats = ErrorStats::default();

        stats.record_error(ErrorCategory::Infrastructure);
        stats.record_error(ErrorCategory::Infrastructure);
        stats.record_error(ErrorCategory::Domain);

        assert_eq!(stats.total_errors, 3);
        assert_eq!(stats.by_category[1], 2); // Infrastructure = 1
        assert_eq!(stats.by_category[3], 1); // Domain = 3

        stats.record_retry(true);
        stats.record_retry(true);
        stats.record_retry(false);

        assert_eq!(stats.total_retries, 3);
        assert_eq!(stats.successful_retries, 2);
        assert!((stats.retry_success_rate() - 0.6666).abs() < 0.01);
    }

    #[test]
    fn test_const_behavior() {
        const BEHAVIOR: ErrorBehaviorWithConfig = default_behavior(ErrorCategory::Infrastructure);
        match BEHAVIOR {
            ErrorBehaviorWithConfig::Retry { config } => {
                assert_eq!(config.max_attempts, 3);
                assert_eq!(config.base_delay_ms, 1000);
            }
            _ => panic!("Expected Retry behavior"),
        }
    }
}
