//!
//! # Error Types
//!
//! Central error types for saga-engine.
//!

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Central error type for saga-engine
#[derive(Debug, Error)]
#[error("{message}")]
pub struct Error {
    /// Error message
    message: String,
    /// Error kind for classification
    kind: ErrorKind,
    /// Source error if any
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    /// Create a new error with message
    pub fn new(message: String, kind: ErrorKind) -> Self {
        Self {
            message,
            kind,
            source: None,
        }
    }

    /// Create from another error
    pub fn from_source<E: std::error::Error + Send + Sync + 'static>(
        message: String,
        kind: ErrorKind,
        source: E,
    ) -> Self {
        Self {
            message,
            kind,
            source: Some(Box::new(source)),
        }
    }

    /// Get error kind
    pub fn kind(&self) -> ErrorKind {
        self.kind.clone()
    }
}

/// Kinds of errors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ErrorKind {
    /// Event store errors
    EventStore,
    /// Codec errors
    Codec,
    /// Workflow execution errors
    WorkflowExecution,
    /// Step execution errors
    StepExecution,
    /// Activity execution errors
    ActivityExecution,
    /// Timer errors
    TimerStore,
    /// Signal dispatcher errors
    SignalDispatcher,
    /// Task queue errors
    TaskQueue,
    /// Snapshot errors
    Snapshot,
    /// Replay errors
    Replay,
    /// Configuration errors
    Configuration,
    /// Validation errors
    Validation,
    /// Timeout errors
    Timeout,
    /// Cancellation errors
    Cancelled,
    /// Concurrency errors
    Concurrency,
    /// Unknown errors
    Unknown,
}

/// Result type with saga-engine error
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_message() {
        let error = Error::new("test error".to_string(), ErrorKind::Unknown);
        assert_eq!(error.to_string(), "test error");
    }

    #[test]
    fn test_error_kind() {
        let error = Error::new("test".to_string(), ErrorKind::EventStore);
        assert_eq!(error.kind(), ErrorKind::EventStore);
    }
}
