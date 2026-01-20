//! Common types for the Saga Port abstraction layer.
//!
//! This module re-exports types from saga-engine-core and provides
//! additional application-specific types.

pub use saga_engine_core::workflow::WorkflowState;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Newtype wrapper for saga execution ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SagaExecutionId(pub String);

impl SagaExecutionId {
    /// Create a new execution ID with a randomly generated UUID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Create an execution ID from an existing string
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the underlying string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SagaExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SagaExecutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Result of a saga port operation that includes metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaPortResult {
    /// The execution ID
    pub execution_id: SagaExecutionId,
    /// The final state
    pub state: WorkflowState,
    /// Timestamp when the operation started
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp when the operation completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Configuration for saga port operations
#[derive(Debug, Clone, Default)]
pub struct SagaPortConfig {
    /// Default timeout for workflow execution
    pub default_timeout: std::time::Duration,
    /// Poll interval for state queries
    pub poll_interval: std::time::Duration,
    /// Maximum retries for transient failures
    pub max_retries: u32,
    /// Retry delay
    pub retry_delay: std::time::Duration,
}

impl SagaPortConfig {
    /// Create a production configuration
    pub fn production() -> Self {
        Self {
            default_timeout: std::time::Duration::from_secs(300),
            poll_interval: std::time::Duration::from_millis(100),
            max_retries: 3,
            retry_delay: std::time::Duration::from_millis(50),
        }
    }

    /// Create a test configuration
    pub fn test() -> Self {
        Self {
            default_timeout: std::time::Duration::from_secs(30),
            poll_interval: std::time::Duration::from_millis(10),
            max_retries: 1,
            retry_delay: std::time::Duration::from_millis(5),
        }
    }
}
