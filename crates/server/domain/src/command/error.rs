// Command Error Types
//
// Error types for command bus operations.

use thiserror::Error;

use saga_engine_core::error::{
    ClassifiedError, ErrorBehaviorWithConfig, ErrorCategory, RetryConfig, RetryOnceConfig,
    default_behavior,
};

/// Error types for command execution
///
/// These errors represent different failure modes that can occur
/// during command processing.
///
/// Errors are categorized as:
/// - **Transient**: Network issues, timeouts, temporary unavailability
/// - **Validation**: Invalid input, missing required fields (not retryable)
/// - **Permanent**: Business rule violations, resource not found
#[derive(Debug, Error, Clone)]
pub enum CommandError {
    /// Handler not found for the command type
    #[error("Handler not found for command type: {command_type}")]
    HandlerNotFound {
        /// The command type that had no handler
        command_type: String,
    },

    /// Handler execution returned an error
    #[error("Handler error for {command_type}: {error}")]
    HandlerError {
        /// The command type that had the error
        command_type: String,
        /// The error message
        error: String,
    },

    /// Command validation failed
    #[error("Command validation failed: {message}")]
    ValidationFailed {
        /// Human-readable validation error message
        message: String,
    },

    /// Command execution failed
    #[error("Command execution failed: {message}")]
    ExecutionFailed {
        /// Human-readable error message
        message: String,
    },

    /// Command already processed (idempotency check)
    #[error("Command already processed (idempotency conflict): {key}")]
    IdempotencyConflict {
        /// The idempotency key that conflicted
        key: String,
    },

    /// Handler panicked during execution
    #[error("Handler panicked during command execution: {command_type}")]
    HandlerPanicked {
        /// The command type that caused the panic
        command_type: String,
    },

    /// Channel closed (bus shutdown)
    #[error("Command bus channel closed")]
    ChannelClosed,

    /// Timeout waiting for command result
    #[error("Command execution timed out after {duration:?}")]
    Timeout {
        /// The timeout duration
        duration: std::time::Duration,
    },

    /// Transient error (network, temporary unavailability)
    #[error("Transient error: {message}")]
    Transient {
        /// Human-readable error message
        message: String,
    },

    /// Resource not found
    #[error("Resource not found: {resource_type}/{resource_id}")]
    NotFound {
        /// Type of resource that was not found
        resource_type: &'static str,
        /// ID of the resource that was not found
        resource_id: String,
    },

    /// Permission denied
    #[error("Permission denied: {message}")]
    PermissionDenied {
        /// Human-readable error message
        message: String,
    },

    /// Type mismatch during command dispatch (internal error)
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// Expected type name
        expected: String,
        /// Actual type name
        actual: String,
    },

    /// Outbox operation error
    #[error("Outbox error for {command_type}: {error}")]
    OutboxError {
        /// The command type that had the error
        command_type: String,
        /// The error message
        error: String,
    },
}

impl CommandError {
    /// Get the command type name for HandlerNotFound errors
    pub fn command_type(&self) -> Option<&String> {
        match self {
            Self::HandlerNotFound { command_type } => Some(command_type),
            Self::HandlerPanicked { command_type } => Some(command_type),
            _ => None,
        }
    }

    /// Returns true if this error is transient and may succeed on retry.
    ///
    /// Transient errors include:
    /// - Network timeouts
    /// - Temporary resource exhaustion
    /// - Channel closed
    #[inline]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Timeout { .. } => true,
            Self::ChannelClosed => true,
            Self::Transient { .. } => true,
            Self::HandlerError { error, .. } => {
                // Check if the error message indicates a transient issue
                let lower = error.to_lowercase();
                lower.contains("timeout")
                    || lower.contains("connection")
                    || lower.contains("temporary")
                    || lower.contains("busy")
                    || lower.contains("unavailable")
            }
            Self::ExecutionFailed { message } => {
                // Check if the error message indicates a transient issue
                let lower = message.to_lowercase();
                lower.contains("timeout")
                    || lower.contains("connection")
                    || lower.contains("temporary")
                    || lower.contains("busy")
                    || lower.contains("unavailable")
            }
            _ => false,
        }
    }

    /// Returns true if this error is a validation error (not retryable).
    ///
    /// Validation errors indicate the command itself is invalid
    /// and retrying will not help.
    #[inline]
    pub fn is_validation(&self) -> bool {
        matches!(self, Self::ValidationFailed { .. })
    }

    /// Returns true if this error indicates a permanent failure.
    ///
    /// Permanent errors should not be retried as they indicate
    /// fundamental issues like permission denied or resource not found.
    #[inline]
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::NotFound { .. }
                | Self::PermissionDenied { .. }
                | Self::IdempotencyConflict { .. }
        )
    }

    /// Creates an ExecutionFailed error from anyhow::Error.
    #[inline]
    pub fn from_anyhow(source: anyhow::Error) -> Self {
        Self::ExecutionFailed {
            message: source.to_string(),
        }
    }

    /// Converts this error to a transient error if it's not already categorized.
    ///
    /// Useful for wrapping external errors into our error type.
    #[inline]
    pub fn into_transient(self) -> Self {
        match self {
            e if e.is_transient() => e,
            Self::ExecutionFailed { message } => Self::Transient { message },
            _ => Self::Transient {
                message: self.to_string(),
            },
        }
    }
}

// ============================================================================
// ClassifiedError Implementation (Saga Engine V4.1)
// ============================================================================

impl ClassifiedError for CommandError {
    fn category(&self) -> ErrorCategory {
        match self {
            // Infrastructure errors (retryable)
            Self::Timeout { .. } => ErrorCategory::TransientDomain,
            Self::ChannelClosed => ErrorCategory::Infrastructure,
            Self::Transient { .. } => ErrorCategory::Infrastructure,
            Self::HandlerError { .. } => ErrorCategory::Infrastructure,
            Self::ExecutionFailed { .. } => ErrorCategory::Infrastructure,
            Self::OutboxError { .. } => ErrorCategory::Infrastructure,
            Self::HandlerPanicked { .. } => ErrorCategory::Fatal,

            // Validation errors (don't retry)
            Self::ValidationFailed { .. } => ErrorCategory::Validation,

            // Domain errors (compensate, don't retry)
            Self::HandlerNotFound { .. } => ErrorCategory::Domain,
            Self::NotFound { .. } => ErrorCategory::Domain,
            Self::PermissionDenied { .. } => ErrorCategory::Domain,
            Self::IdempotencyConflict { .. } => ErrorCategory::Domain,
            Self::TypeMismatch { .. } => ErrorCategory::Domain,
        }
    }

    fn behavior(&self) -> ErrorBehaviorWithConfig {
        match self {
            Self::Timeout { duration, .. } => ErrorBehaviorWithConfig::RetryOnce {
                config: RetryOnceConfig {
                    delay_ms: duration.as_millis() as u64,
                },
            },
            Self::ChannelClosed => ErrorBehaviorWithConfig::Retry {
                config: RetryConfig {
                    max_attempts: 3,
                    base_delay_ms: 1000,
                },
            },
            Self::HandlerPanicked { .. } => ErrorBehaviorWithConfig::Fail,
            Self::TypeMismatch { .. } => ErrorBehaviorWithConfig::Fail,
            _ => default_behavior(self.category()),
        }
    }

    fn retry_config(&self) -> Option<RetryConfig> {
        match self {
            Self::HandlerError { .. } => Some(RetryConfig {
                max_attempts: 3,
                base_delay_ms: 500,
            }),
            Self::OutboxError { .. } => Some(RetryConfig {
                max_attempts: 3,
                base_delay_ms: 200,
            }),
            _ => None,
        }
    }
}

/// Result type for command dispatch
pub type CommandResult<T, E = CommandError> = Result<T, E>;
