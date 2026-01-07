// Command Error Types
//
// Error types for command bus operations.

use thiserror::Error;

/// Error types for command execution
///
/// These errors represent different failure modes that can occur
/// during command processing.
#[derive(Debug, Error)]
pub enum CommandError {
    /// Handler not found for the command type
    #[error("Handler not found for command type: {command_type}")]
    HandlerNotFound {
        /// The command type that had no handler
        command_type: &'static str,
    },

    /// Command validation failed
    #[error("Command validation failed: {message}")]
    ValidationFailed {
        /// Human-readable validation error message
        message: String,
    },

    /// Command execution failed
    #[error("Command execution failed: {source}")]
    ExecutionFailed {
        /// The underlying error that caused the failure
        #[from]
        source: anyhow::Error,
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
        command_type: &'static str,
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
}

impl CommandError {
    /// Get the command type name for HandlerNotFound errors
    pub fn command_type(&self) -> Option<&'static str> {
        match self {
            Self::HandlerNotFound { command_type } => Some(command_type),
            Self::HandlerPanicked { command_type } => Some(command_type),
            _ => None,
        }
    }
}

/// Result type for command dispatch
pub type CommandResult<T, E = CommandError> = Result<T, E>;
