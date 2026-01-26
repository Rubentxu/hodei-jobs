//!
//! # Error Types
//!
//! Central error types for saga-engine with rich context for debugging.
//!

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use thiserror::Error;

/// Central error type for saga-engine
#[derive(Debug)]
pub struct Error {
    /// Error message
    message: String,
    /// Error kind for classification
    kind: ErrorKind,
    /// Source error if any
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    /// Timestamp when error occurred
    timestamp: SystemTime,
    /// Additional context attributes
    context: HashMap<String, String>,
    /// Stack trace snippet (if available)
    stack_trace: Option<String>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| &**e as &(dyn std::error::Error + 'static))
    }
}

impl Error {
    /// Create a new error with message
    pub fn new(message: String, kind: ErrorKind) -> Self {
        Self {
            message,
            kind,
            source: None,
            timestamp: SystemTime::now(),
            context: HashMap::new(),
            stack_trace: None,
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
            timestamp: SystemTime::now(),
            context: HashMap::new(),
            stack_trace: None,
        }
    }

    /// Get error kind
    pub fn kind(&self) -> ErrorKind {
        self.kind.clone()
    }

    /// Add context attribute
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Get context value
    pub fn get_context(&self, key: &str) -> Option<&str> {
        self.context.get(key).map(|s| s.as_str())
    }

    /// Get all context
    pub fn context(&self) -> &HashMap<String, String> {
        &self.context
    }

    /// Get timestamp
    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }

    /// Set stack trace
    pub fn with_stack_trace(mut self, stack_trace: impl Into<String>) -> Self {
        self.stack_trace = Some(stack_trace.into());
        self
    }

    /// Get stack trace
    pub fn stack_trace(&self) -> Option<&str> {
        self.stack_trace.as_deref()
    }

    /// Convert to structured format
    pub fn to_structured(&self) -> StructuredError {
        StructuredError {
            message: self.message.clone(),
            kind: self.kind.clone(),
            timestamp: chrono::DateTime::from(self.timestamp),
            context: self.context.clone(),
            source: self.source.as_ref().map(|e| e.to_string()),
        }
    }
}

/// Structured error for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredError {
    pub message: String,
    pub kind: ErrorKind,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub context: HashMap<String, String>,
    pub source: Option<String>,
}

/// Kinds of errors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ErrorKind {
    /// Event store errors
    #[serde(rename = "event_store")]
    EventStore,
    /// Codec errors
    #[serde(rename = "codec")]
    Codec,
    /// Workflow execution errors
    #[serde(rename = "workflow_execution")]
    WorkflowExecution,
    /// Step execution errors
    #[serde(rename = "step_execution")]
    StepExecution,
    /// Activity execution errors
    #[serde(rename = "activity_execution")]
    ActivityExecution,
    /// Timer errors
    #[serde(rename = "timer_store")]
    TimerStore,
    /// Signal dispatcher errors
    #[serde(rename = "signal_dispatcher")]
    SignalDispatcher,
    /// Task queue errors
    #[serde(rename = "task_queue")]
    TaskQueue,
    /// Snapshot errors
    #[serde(rename = "snapshot")]
    Snapshot,
    /// Replay errors
    #[serde(rename = "replay")]
    Replay,
    /// Configuration errors
    #[serde(rename = "configuration")]
    Configuration,
    /// Validation errors
    #[serde(rename = "validation")]
    Validation,
    /// Timeout errors
    #[serde(rename = "timeout")]
    Timeout,
    /// Cancellation errors
    #[serde(rename = "cancelled")]
    Cancelled,
    /// Concurrency errors
    #[serde(rename = "concurrency")]
    Concurrency,
    /// Compensation errors
    #[serde(rename = "compensation")]
    Compensation,
    /// Unknown errors
    #[serde(rename = "unknown")]
    Unknown,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::EventStore => write!(f, "event_store"),
            ErrorKind::Codec => write!(f, "codec"),
            ErrorKind::WorkflowExecution => write!(f, "workflow_execution"),
            ErrorKind::StepExecution => write!(f, "step_execution"),
            ErrorKind::ActivityExecution => write!(f, "activity_execution"),
            ErrorKind::TimerStore => write!(f, "timer_store"),
            ErrorKind::SignalDispatcher => write!(f, "signal_dispatcher"),
            ErrorKind::TaskQueue => write!(f, "task_queue"),
            ErrorKind::Snapshot => write!(f, "snapshot"),
            ErrorKind::Replay => write!(f, "replay"),
            ErrorKind::Configuration => write!(f, "configuration"),
            ErrorKind::Validation => write!(f, "validation"),
            ErrorKind::Timeout => write!(f, "timeout"),
            ErrorKind::Cancelled => write!(f, "cancelled"),
            ErrorKind::Concurrency => write!(f, "concurrency"),
            ErrorKind::Compensation => write!(f, "compensation"),
            ErrorKind::Unknown => write!(f, "unknown"),
        }
    }
}

/// Result type with saga-engine error
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Helper macro to create errors with context
#[macro_export]
macro_rules! ctx_error {
    ($kind:expr, $message:expr) => {
        $crate::Error::new($message.to_string(), $kind)
    };
    ($kind:expr, $message:expr, $($key:expr => $value:expr),+) => {
        {
            let mut error = $crate::Error::new($message.to_string(), $kind);
            $(
                error = error.with_context($key, $value);
            )+
            error
        }
    };
}

/// Helper functions for common error scenarios
impl Error {
    /// Create an event store error
    pub fn event_store(message: impl Into<String>) -> Self {
        Self::new(message.into(), ErrorKind::EventStore)
    }

    /// Create a workflow execution error
    pub fn workflow_execution(message: impl Into<String>) -> Self {
        Self::new(message.into(), ErrorKind::WorkflowExecution)
    }

    /// Create a step execution error
    pub fn step_execution(message: impl Into<String>) -> Self {
        Self::new(message.into(), ErrorKind::StepExecution)
    }

    /// Create an activity execution error
    pub fn activity_execution(message: impl Into<String>) -> Self {
        Self::new(message.into(), ErrorKind::ActivityExecution)
    }

    /// Create a timeout error
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(message.into(), ErrorKind::Timeout)
    }

    /// Create a cancellation error
    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new(message.into(), ErrorKind::Cancelled)
    }

    /// Create a compensation error
    pub fn compensation(message: impl Into<String>) -> Self {
        Self::new(message.into(), ErrorKind::Compensation)
    }

    /// Create a validation error
    pub fn validation(field: &str, message: &str) -> Self {
        Self::new(
            format!("Validation error on field '{}': {}", field, message),
            ErrorKind::Validation,
        )
        .with_context("field", field)
    }
}

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

    #[test]
    fn test_error_context() {
        let error = Error::new("test".to_string(), ErrorKind::WorkflowExecution)
            .with_context("saga_id", "saga-123")
            .with_context("step", "step_1");

        assert_eq!(error.get_context("saga_id"), Some("saga-123"));
        assert_eq!(error.get_context("step"), Some("step_1"));
        assert_eq!(error.get_context("nonexistent"), None);
    }

    #[test]
    fn test_structured_error() {
        let error = Error::new("test error".to_string(), ErrorKind::ActivityExecution)
            .with_context("activity_type", "activity_a");

        let structured = error.to_structured();
        assert_eq!(structured.message, "test error");
        assert_eq!(structured.kind, ErrorKind::ActivityExecution);
        assert_eq!(
            structured.context.get("activity_type"),
            Some(&"activity_a".to_string())
        );
    }

    #[test]
    fn test_helper_constructors() {
        let error1 = Error::event_store("connection failed");
        assert_eq!(error1.kind(), ErrorKind::EventStore);

        let error2 = Error::workflow_execution("saga failed");
        assert_eq!(error2.kind(), ErrorKind::WorkflowExecution);

        let error3 = Error::validation("email", "invalid format");
        assert_eq!(error3.kind(), ErrorKind::Validation);
        assert_eq!(error3.get_context("field"), Some("email"));
    }
}
