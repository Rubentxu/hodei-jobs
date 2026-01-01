//! Provider Connection Errors
//!
//! Typed errors for provider connection failures with retry classification.

use std::time::Duration;
use thiserror::Error;

/// Classification of provider connection errors
///
/// This enum provides detailed error classification to enable:
/// - Appropriate retry strategies
/// - Clear error messages for operators
/// - Distinction between transient and permanent failures
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderConnectionError {
    /// Configuration is invalid or missing required fields
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Authentication failed (invalid credentials, expired token)
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Authorization denied (insufficient permissions)
    #[error("Authorization denied: {0}")]
    Authorization(String),

    /// Network connectivity issue
    #[error("Network error: {0}")]
    Network(String),

    /// Operation timed out
    #[error("Timeout after {timeout}s: {message}")]
    Timeout { timeout: u64, message: String },

    /// Resource quota exceeded
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// API returned an error response
    #[error("API error: {0}")]
    ApiError(String),

    /// Provider is not reachable (host unreachable, DNS failure)
    #[error("Provider unreachable: {0}")]
    Unreachable(String),

    /// TLS/SSL certificate validation failed
    #[error("TLS error: {0}")]
    TlsError(String),

    /// Unknown error occurred
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl ProviderConnectionError {
    /// Returns true if the error is transient and may resolve with retry
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::Timeout { .. } | Self::ApiError(_) | Self::Unreachable(_) => {
                true
            }
            Self::Configuration(_)
            | Self::Authentication(_)
            | Self::Authorization(_)
            | Self::ResourceExhausted(_)
            | Self::TlsError(_) => false,
            Self::Unknown(_) => true, // Assume retryable by default
        }
    }

    /// Returns the recommended retry strategy for this error
    pub fn retry_strategy(&self) -> RetryStrategy {
        match self {
            Self::Network(_) => RetryStrategy::ExponentialBackoff {
                max_attempts: 3,
                base_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(10),
            },
            Self::Timeout { timeout, .. } => RetryStrategy::ExponentialBackoff {
                max_attempts: 2,
                base_delay: Duration::from_secs(*timeout),
                max_delay: Duration::from_secs(60),
            },
            Self::Unreachable(_) => RetryStrategy::ExponentialBackoff {
                max_attempts: 3,
                base_delay: Duration::from_secs(2),
                max_delay: Duration::from_secs(30),
            },
            Self::ApiError(_) => RetryStrategy::ExponentialBackoff {
                max_attempts: 2,
                base_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(5),
            },
            Self::ResourceExhausted(_) => RetryStrategy::LinearBackoff {
                max_attempts: 1,
                delay: Duration::from_secs(60),
            },
            _ => RetryStrategy::None,
        }
    }

    /// Returns a user-friendly remediation suggestion
    pub fn remediation_suggestion(&self) -> String {
        match self {
            Self::Configuration(msg) => format!("Check provider configuration: {}", msg),
            Self::Authentication(msg) => format!("Update credentials: {}", msg),
            Self::Authorization(msg) => format!("Fix RBAC permissions: {}", msg),
            Self::Network(msg) => format!("Verify network connectivity: {}", msg),
            Self::Timeout { timeout, message } => {
                format!("Increase timeout (current: {}s): {}", timeout, message)
            }
            Self::ResourceExhausted(msg) => {
                format!("Free up resources or request quota increase: {}", msg)
            }
            Self::ApiError(msg) => format!("Check provider API status: {}", msg),
            Self::Unreachable(msg) => format!("Verify provider endpoint: {}", msg),
            Self::TlsError(msg) => format!("Update TLS certificates: {}", msg),
            Self::Unknown(msg) => format!("Review logs for details: {}", msg),
        }
    }
}

/// Retry strategy for transient failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryStrategy {
    /// No retry should be attempted
    None,
    /// Linear backoff with fixed delay between attempts
    LinearBackoff {
        /// Maximum number of attempts
        max_attempts: u32,
        /// Delay between attempts
        delay: Duration,
    },
    /// Exponential backoff with increasing delays
    ExponentialBackoff {
        /// Maximum number of attempts
        max_attempts: u32,
        /// Base delay for first retry
        base_delay: Duration,
        /// Maximum delay cap
        max_delay: Duration,
    },
}

impl RetryStrategy {
    /// Returns the maximum number of retry attempts
    pub fn max_attempts(&self) -> u32 {
        match self {
            Self::None => 0,
            Self::LinearBackoff { max_attempts, .. } => *max_attempts,
            Self::ExponentialBackoff { max_attempts, .. } => *max_attempts,
        }
    }

    /// Calculate delay for a given attempt number (0-indexed)
    pub fn delay_for_attempt(&self, attempt: u32) -> Option<Duration> {
        match self {
            Self::None => None,
            Self::LinearBackoff { delay, .. } => Some(*delay),
            Self::ExponentialBackoff {
                base_delay,
                max_delay,
                ..
            } => {
                let base_secs = base_delay.as_secs();
                let max_secs = max_delay.as_secs();
                let exp_delay_secs = base_secs.saturating_mul(2_u64.pow(attempt));
                Some(Duration::from_secs(std::cmp::min(exp_delay_secs, max_secs)))
            }
        }
    }
}

/// Result type for provider connection validation
pub type ValidationResult = std::result::Result<(), ProviderConnectionError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(ProviderConnectionError::Network("connection refused".to_string()).is_retryable());
        assert!(
            ProviderConnectionError::Timeout {
                timeout: 10,
                message: "test".to_string()
            }
            .is_retryable()
        );
        assert!(ProviderConnectionError::ApiError("500".to_string()).is_retryable());
        assert!(ProviderConnectionError::Unreachable("host down".to_string()).is_retryable());
    }

    #[test]
    fn test_non_retryable_errors() {
        assert!(
            !ProviderConnectionError::Configuration("missing field".to_string()).is_retryable()
        );
        assert!(
            !ProviderConnectionError::Authentication("invalid token".to_string()).is_retryable()
        );
        assert!(!ProviderConnectionError::Authorization("forbidden".to_string()).is_retryable());
        assert!(
            !ProviderConnectionError::ResourceExhausted("quota exceeded".to_string())
                .is_retryable()
        );
        assert!(!ProviderConnectionError::TlsError("cert expired".to_string()).is_retryable());
    }

    #[test]
    fn test_retry_strategy_delays() {
        let strategy = RetryStrategy::ExponentialBackoff {
            max_attempts: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        };

        assert_eq!(strategy.delay_for_attempt(0), Some(Duration::from_secs(1)));
        assert_eq!(strategy.delay_for_attempt(1), Some(Duration::from_secs(2)));
        assert_eq!(strategy.delay_for_attempt(2), Some(Duration::from_secs(4)));
        assert_eq!(strategy.delay_for_attempt(3), Some(Duration::from_secs(8)));
        assert_eq!(strategy.delay_for_attempt(4), Some(Duration::from_secs(16)));
        assert_eq!(strategy.delay_for_attempt(5), Some(Duration::from_secs(30))); // capped
    }

    #[test]
    fn test_remediation_suggestions() {
        let config_error = ProviderConnectionError::Configuration("missing namespace".to_string());
        assert!(
            config_error
                .remediation_suggestion()
                .contains("Check provider configuration")
        );

        let auth_error = ProviderConnectionError::Authentication("invalid token".to_string());
        assert!(
            auth_error
                .remediation_suggestion()
                .contains("Update credentials")
        );
    }
}
