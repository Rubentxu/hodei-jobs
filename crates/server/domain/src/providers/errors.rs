//! Provider Connection and Initialization Errors
//!
//! Typed errors for provider connection failures with retry classification,
//! and initialization errors for startup validation.

use std::time::Duration;
use thiserror::Error;

use crate::shared_kernel::{DomainError, ProviderId};
use crate::workers::ProviderType;

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

/// Errors that can occur during provider initialization at startup.
///
/// This enum provides detailed error information for:
/// - Fail-fast startup validation
/// - Clear remediation instructions for operators
/// - Integration with monitoring and alerting
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderInitializationError {
    /// No providers found in the database with status 'Active'
    #[error(
        "No active providers configured. \
         At least one provider is required to execute jobs. \
         See: https://hodei-jobs.io/docs/providers/kubernetes-setup"
    )]
    NoActiveProviders { details: String },

    /// Provider configuration not found in database
    #[error("Provider configuration not found: {provider_id}")]
    ProviderNotFound { provider_id: ProviderId },

    /// Provider configuration is invalid
    #[error("Invalid provider configuration for '{provider_id}': {reason}")]
    InvalidConfiguration {
        provider_id: ProviderId,
        provider_type: ProviderType,
        reason: String,
    },

    /// Failed to connect to the provider infrastructure
    #[error("Failed to connect to {provider_type} provider '{provider_id}': {connection_error}")]
    ConnectionFailed {
        provider_id: ProviderId,
        provider_type: ProviderType,
        connection_error: ProviderConnectionError,
    },

    /// RBAC permissions validation failed
    #[error(
        "Insufficient RBAC permissions for Kubernetes provider '{provider_id}'. \
         Missing permissions: {missing_permissions:?}. \
         Run: kubectl apply -f https://hodei-jobs.io/manifests/rbac.yaml"
    )]
    RbacValidationFailed {
        provider_id: ProviderId,
        missing_permissions: Vec<String>,
    },

    /// Provider validation produced warnings (non-fatal)
    #[error(
        "Provider '{provider_id}' initialized with {warnings_count} warning(s). \
         Some features may be limited."
    )]
    ValidationWarnings {
        provider_id: ProviderId,
        provider_type: ProviderType,
        warnings_count: usize,
    },

    /// Provider initialization timed out
    #[error(
        "Provider '{provider_id}' initialization timed out after {timeout_secs}s. \
         Check infrastructure connectivity."
    )]
    InitializationTimeout {
        provider_id: ProviderId,
        provider_type: ProviderType,
        timeout_secs: u64,
    },

    /// Unexpected error during initialization
    #[error("Unexpected error initializing provider '{provider_id}': {source}")]
    UnexpectedError {
        provider_id: ProviderId,
        provider_type: ProviderType,
        source: ErrorMessage,
    },
}

/// Wrapper for error messages that implements Display, Clone, Eq, PartialEq
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorMessage(pub String);

impl std::fmt::Display for ErrorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ErrorMessage {}

impl From<String> for ErrorMessage {
    fn from(s: String) -> Self {
        ErrorMessage(s)
    }
}

impl From<&str> for ErrorMessage {
    fn from(s: &str) -> Self {
        ErrorMessage(s.to_string())
    }
}

impl ProviderInitializationError {
    /// Returns a user-friendly message for operators
    pub fn to_user_message(&self) -> String {
        self.to_string()
    }

    /// Returns the provider type associated with this error, if any
    pub fn provider_type(&self) -> Option<ProviderType> {
        match self {
            Self::NoActiveProviders { .. } => None,
            Self::ProviderNotFound { .. } => None,
            Self::InvalidConfiguration { provider_type, .. } => Some(provider_type.clone()),
            Self::ConnectionFailed { provider_type, .. } => Some(provider_type.clone()),
            Self::RbacValidationFailed { .. } => Some(ProviderType::Kubernetes),
            Self::ValidationWarnings { provider_type, .. } => Some(provider_type.clone()),
            Self::InitializationTimeout { provider_type, .. } => Some(provider_type.clone()),
            Self::UnexpectedError { provider_type, .. } => Some(provider_type.clone()),
        }
    }

    /// Returns true if this error should cause startup to fail
    ///
    /// Some errors are fatal (no providers available), while others
    /// allow the system to start with reduced capacity.
    pub fn is_fatal(&self) -> bool {
        match self {
            Self::NoActiveProviders { .. } => true,
            Self::ProviderNotFound { .. } => false,
            Self::InvalidConfiguration { .. } => true,
            Self::ConnectionFailed { .. } => false, // Try other providers
            Self::RbacValidationFailed { .. } => false, // Warning with remediation
            Self::ValidationWarnings { .. } => false,
            Self::InitializationTimeout { .. } => false, // Try other providers
            Self::UnexpectedError { .. } => false,
        }
    }

    /// Returns a user-friendly remediation suggestion
    pub fn remediation_suggestion(&self) -> String {
        match self {
            Self::NoActiveProviders { details: _ } => {
                "To configure a Kubernetes provider, create a ConfigMap in PostgreSQL: \
                 INSERT INTO provider_configs (id, name, provider_type, type_config, status) \
                 VALUES ('k8s-default', 'kubernetes-default', 'kubernetes', \
                 '{\"namespace\": \"hodei-jobs-workers\"}', 'active'); \
                 See: https://hodei-jobs.io/docs/providers/kubernetes-setup"
                    .to_string()
            }
            Self::ProviderNotFound { provider_id } => {
                format!(
                    "Provider '{}' was expected but not found in the database. \
                     Verify the provider was created and is active.",
                    provider_id
                )
            }
            Self::InvalidConfiguration {
                provider_id,
                reason,
                ..
            } => {
                format!(
                    "Check configuration for provider '{}': {}. \
                     Review the provider configuration for missing or invalid fields.",
                    provider_id, reason
                )
            }
            Self::ConnectionFailed {
                provider_id,
                connection_error,
                ..
            } => {
                format!(
                    "Verify connectivity to provider '{}': {}. \
                     Check network configuration and provider endpoints.",
                    provider_id,
                    connection_error.remediation_suggestion()
                )
            }
            Self::RbacValidationFailed {
                provider_id,
                missing_permissions,
                ..
            } => {
                format!(
                    "Apply RBAC permissions for provider '{}':\n\
                     kubectl apply -f https://hodei-jobs.io/manifests/rbac.yaml\n\n\
                     Missing permissions: {}",
                    provider_id,
                    missing_permissions.join(", ")
                )
            }
            Self::ValidationWarnings {
                provider_id,
                provider_type: _,
                warnings_count,
            } => {
                format!(
                    "Provider '{}' has {} warning(s). Review logs for details.",
                    provider_id, warnings_count
                )
            }
            Self::InitializationTimeout {
                provider_id,
                timeout_secs,
                ..
            } => {
                format!(
                    "Increase timeout for provider '{}' (current: {}s) \
                     or check infrastructure connectivity.",
                    provider_id, timeout_secs
                )
            }
            Self::UnexpectedError {
                provider_id,
                provider_type: _,
                source,
            } => {
                format!(
                    "Check logs for details on provider '{}': {}",
                    provider_id, source
                )
            }
        }
    }

    /// Returns the provider ID associated with this error, if any
    pub fn provider_id(&self) -> Option<&ProviderId> {
        match self {
            Self::NoActiveProviders { .. } => None,
            Self::ProviderNotFound { provider_id, .. } => Some(provider_id),
            Self::InvalidConfiguration { provider_id, .. } => Some(provider_id),
            Self::ConnectionFailed { provider_id, .. } => Some(provider_id),
            Self::RbacValidationFailed { provider_id, .. } => Some(provider_id),
            Self::ValidationWarnings { provider_id, .. } => Some(provider_id),
            Self::InitializationTimeout { provider_id, .. } => Some(provider_id),
            Self::UnexpectedError { provider_id, .. } => Some(provider_id),
        }
    }
}

/// Result type for provider initialization
pub type InitializationResult<T = ()> = std::result::Result<T, ProviderInitializationError>;

/// Result type alias for provider initialization (alias for backward compatibility)
pub type ProviderInitializationResult = InitializationResult<()>;

/// Implement From<ProviderInitializationError> for DomainError
impl From<ProviderInitializationError> for DomainError {
    fn from(error: ProviderInitializationError) -> Self {
        match error {
            ProviderInitializationError::NoActiveProviders { details } => {
                DomainError::InvalidProviderConfig {
                    message: format!("No active providers configured. {}", details),
                }
            }
            ProviderInitializationError::ProviderNotFound { provider_id, .. } => {
                DomainError::ProviderNotFound { provider_id }
            }
            ProviderInitializationError::InvalidConfiguration {
                provider_id,
                reason,
                ..
            } => DomainError::InvalidProviderConfig {
                message: format!("Invalid configuration for {}: {}", provider_id, reason),
            },
            ProviderInitializationError::ConnectionFailed {
                provider_id,
                connection_error,
                ..
            } => DomainError::InfrastructureError {
                message: format!(
                    "Connection failed for provider {}: {}",
                    provider_id, connection_error
                ),
            },
            ProviderInitializationError::RbacValidationFailed {
                provider_id,
                missing_permissions,
                ..
            } => DomainError::InvalidProviderConfig {
                message: format!(
                    "RBAC validation failed for provider {}: missing permissions {:?}",
                    provider_id, missing_permissions
                ),
            },
            ProviderInitializationError::ValidationWarnings { provider_id, .. } => {
                DomainError::InvalidProviderConfig {
                    message: format!("Provider {} initialized with warnings", provider_id),
                }
            }
            ProviderInitializationError::InitializationTimeout {
                provider_id,
                timeout_secs,
                ..
            } => DomainError::InfrastructureError {
                message: format!(
                    "Provider {} initialization timed out after {}s",
                    provider_id, timeout_secs
                ),
            },
            ProviderInitializationError::UnexpectedError {
                provider_id,
                source,
                ..
            } => DomainError::InfrastructureError {
                message: format!(
                    "Unexpected error initializing provider {}: {}",
                    provider_id, source
                ),
            },
        }
    }
}

/// Summary of provider initialization results
#[derive(Debug, Clone, Default)]
pub struct InitializationSummary {
    /// Total providers processed
    pub total_processed: usize,
    /// Providers successfully initialized
    pub successful: Vec<ProviderId>,
    /// Providers that failed initialization
    pub failed: Vec<(ProviderId, ProviderInitializationError)>,
    /// Providers with warnings
    pub warnings: Vec<(ProviderId, Vec<String>)>,
    /// Total initialization duration
    pub duration_ms: u64,
}

impl InitializationSummary {
    /// Create a new initialization summary
    pub fn new(
        total_processed: usize,
        successful: Vec<ProviderId>,
        failed: Vec<(ProviderId, ProviderInitializationError)>,
        warnings: Vec<(ProviderId, Vec<String>)>,
        duration_ms: u64,
    ) -> Self {
        Self {
            total_processed,
            successful,
            failed,
            warnings,
            duration_ms,
        }
    }

    /// Returns true if at least one provider was successfully initialized
    pub fn has_successful_providers(&self) -> bool {
        !self.successful.is_empty()
    }

    /// Returns true if all providers failed
    pub fn all_failed(&self) -> bool {
        self.successful.is_empty() && !self.failed.is_empty()
    }

    /// Returns a formatted summary string
    pub fn summary(&self) -> String {
        format!(
            "Provider initialization: {} total, {} successful, {} failed, {} warnings, {}ms",
            self.total_processed,
            self.successful.len(),
            self.failed.len(),
            self.warnings.len(),
            self.duration_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::ProviderId;
    use crate::workers::ProviderType;

    /// Helper to create a test provider ID
    fn test_provider_id() -> ProviderId {
        ProviderId::new()
    }

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

    // ProviderInitializationError tests
    #[test]
    fn test_initialization_error_is_fatal() {
        let fatal_errors: Vec<ProviderInitializationError> = vec![
            ProviderInitializationError::NoActiveProviders {
                details: "test".to_string(),
            },
            ProviderInitializationError::InvalidConfiguration {
                provider_id: test_provider_id(),
                provider_type: ProviderType::Kubernetes,
                reason: "invalid config".to_string(),
            },
        ];

        let non_fatal_errors: Vec<ProviderInitializationError> = vec![
            ProviderInitializationError::ProviderNotFound {
                provider_id: test_provider_id(),
            },
            ProviderInitializationError::ConnectionFailed {
                provider_id: test_provider_id(),
                provider_type: ProviderType::Kubernetes,
                connection_error: ProviderConnectionError::Network("test".to_string()),
            },
            ProviderInitializationError::RbacValidationFailed {
                provider_id: test_provider_id(),
                missing_permissions: vec!["pods/create".to_string()],
            },
            ProviderInitializationError::ValidationWarnings {
                provider_id: test_provider_id(),
                provider_type: ProviderType::Kubernetes,
                warnings_count: 1,
            },
            ProviderInitializationError::InitializationTimeout {
                provider_id: test_provider_id(),
                provider_type: ProviderType::Kubernetes,
                timeout_secs: 30,
            },
            ProviderInitializationError::UnexpectedError {
                provider_id: test_provider_id(),
                provider_type: ProviderType::Kubernetes,
                source: ErrorMessage::from("unknown error"),
            },
        ];

        for error in fatal_errors {
            assert!(error.is_fatal(), "{:?} should be fatal", error);
        }

        for error in non_fatal_errors {
            assert!(!error.is_fatal(), "{:?} should not be fatal", error);
        }
    }

    #[test]
    fn test_initialization_error_provider_id() {
        let provider_id = test_provider_id();

        let errors_with_id: Vec<ProviderInitializationError> = vec![
            ProviderInitializationError::ProviderNotFound {
                provider_id: provider_id.clone(),
            },
            ProviderInitializationError::InvalidConfiguration {
                provider_id: provider_id.clone(),
                provider_type: ProviderType::Kubernetes,
                reason: "test".to_string(),
            },
            ProviderInitializationError::ConnectionFailed {
                provider_id: provider_id.clone(),
                provider_type: ProviderType::Kubernetes,
                connection_error: ProviderConnectionError::Network("test".to_string()),
            },
            ProviderInitializationError::RbacValidationFailed {
                provider_id: provider_id.clone(),
                missing_permissions: vec![],
            },
            ProviderInitializationError::ValidationWarnings {
                provider_id: provider_id.clone(),
                provider_type: ProviderType::Kubernetes,
                warnings_count: 0,
            },
            ProviderInitializationError::InitializationTimeout {
                provider_id: provider_id.clone(),
                provider_type: ProviderType::Kubernetes,
                timeout_secs: 30,
            },
            ProviderInitializationError::UnexpectedError {
                provider_id: provider_id.clone(),
                provider_type: ProviderType::Kubernetes,
                source: ErrorMessage::from("test"),
            },
        ];

        for error in errors_with_id {
            assert_eq!(error.provider_id(), Some(&provider_id));
        }

        assert_eq!(
            ProviderInitializationError::NoActiveProviders {
                details: "test".to_string()
            }
            .provider_id(),
            None
        );
    }

    #[test]
    fn test_initialization_summary() {
        let provider_id1 = test_provider_id();
        let provider_id2 = test_provider_id();

        let summary = InitializationSummary {
            total_processed: 3,
            successful: vec![provider_id1.clone()],
            failed: vec![(
                provider_id2.clone(),
                ProviderInitializationError::ConnectionFailed {
                    provider_id: provider_id2.clone(),
                    provider_type: ProviderType::Kubernetes,
                    connection_error: ProviderConnectionError::Network("test".to_string()),
                },
            )],
            warnings: vec![],
            duration_ms: 150,
        };

        assert!(summary.has_successful_providers());
        assert!(!summary.all_failed());

        let summary_empty = InitializationSummary::default();
        assert!(!summary_empty.has_successful_providers());
        assert!(!summary_empty.all_failed());

        let summary_all_failed = InitializationSummary {
            total_processed: 2,
            successful: vec![],
            failed: vec![(
                provider_id1.clone(),
                ProviderInitializationError::ConnectionFailed {
                    provider_id: provider_id1.clone(),
                    provider_type: ProviderType::Kubernetes,
                    connection_error: ProviderConnectionError::Network("test".to_string()),
                },
            )],
            warnings: vec![],
            duration_ms: 100,
        };
        assert!(summary_all_failed.all_failed());

        let summary_str = summary.summary();
        assert!(summary_str.contains("Provider initialization:"));
        assert!(summary_str.contains("1 successful"));
        assert!(summary_str.contains("1 failed"));
        assert!(summary_str.contains("150ms"));
    }
}
