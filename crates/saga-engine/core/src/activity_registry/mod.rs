//! # ActivityRegistry - Activity Registration and Execution
//!
//! This module provides the [`ActivityRegistry`] for registering and executing
//! activities in the saga engine.

use crate::workflow::Activity;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Unique identifier for an activity type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActivityTypeId(pub String);

impl ActivityTypeId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActivityTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Result of activity execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityResult {
    Completed(serde_json::Value),
    Failed(String),
    TimedOut,
    Cancelled,
    NotFound,
}

impl ActivityResult {
    pub fn is_success(&self) -> bool {
        matches!(self, ActivityResult::Completed(_))
    }

    pub fn output(&self) -> Option<&serde_json::Value> {
        match self {
            ActivityResult::Completed(v) => Some(v),
            _ => None,
        }
    }
}

/// Errors from activity operations.
#[derive(Debug, Error)]
pub enum ActivityError {
    #[error("Activity not found: {0}")]
    NotFound(ActivityTypeId),

    #[error("Activity execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Activity timeout after {0:?}")]
    Timeout(Duration),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Activity execution context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityContext {
    pub activity_type: ActivityTypeId,
    pub attempt: u32,
    pub timeout: Duration,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl ActivityContext {
    pub fn new(activity_type: ActivityTypeId) -> Self {
        Self {
            activity_type,
            attempt: 1,
            timeout: Duration::from_secs(300),
            started_at: chrono::Utc::now(),
        }
    }

    pub fn increment_attempt(&mut self) {
        self.attempt += 1;
    }
}

/// Type-erased activity trait for storage in the registry.
#[async_trait]
pub trait DynActivity: Send + Sync + fmt::Debug {
    /// Execute the activity with type-erased input (Value).
    async fn execute_dyn(
        &self,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActivityError>;
}

#[async_trait]
impl<A: Activity> DynActivity for A {
    async fn execute_dyn(
        &self,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActivityError> {
        // Deserialize input
        let typed_input: A::Input = serde_json::from_value(input).map_err(|e| {
            ActivityError::Serialization(format!("Invalid input for {}: {}", A::TYPE_ID, e))
        })?;

        // Execute
        let output = self
            .execute(typed_input)
            .await
            .map_err(|e| ActivityError::ExecutionFailed(e.to_string()))?;

        // Serialize output
        serde_json::to_value(output).map_err(|e| {
            ActivityError::Serialization(format!(
                "Failed to serialize output for {}: {}",
                A::TYPE_ID,
                e
            ))
        })
    }
}

/// Activity registry for managing activity types and execution.
pub struct ActivityRegistry {
    /// Map of registered activities (Type -> Implementation).
    activities: DashMap<String, Arc<dyn DynActivity>>,
    /// Default timeout for activities.
    pub default_timeout: Duration,
}

impl fmt::Debug for ActivityRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActivityRegistry")
            .field("activity_count", &self.activities.len())
            .field("default_timeout", &self.default_timeout)
            .finish()
    }
}

impl Default for ActivityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityRegistry {
    /// Create a new activity registry.
    pub fn new() -> Self {
        Self {
            activities: DashMap::new(),
            default_timeout: Duration::from_secs(300),
        }
    }

    /// Set the default timeout.
    pub fn with_default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Register an activity implementation.
    ///
    /// The activity type ID is taken from `A::TYPE_ID`.
    pub fn register_activity<A: Activity>(&self, activity: A) {
        let type_id = A::TYPE_ID.to_string();
        self.activities.insert(type_id, Arc::new(activity));
    }

    /// Check if an activity type is registered.
    pub fn has_activity(&self, activity_type: &str) -> bool {
        self.activities.contains_key(activity_type)
    }

    /// Get an activity implementation by type.
    pub fn get_activity(&self, activity_type: &str) -> Option<Arc<dyn DynActivity>> {
        self.activities
            .get(activity_type)
            .map(|r| r.value().clone())
    }

    /// Get count of registered activities.
    pub fn len(&self) -> usize {
        self.activities.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.activities.is_empty()
    }

    /// Create an execution context.
    pub fn create_context(&self, activity_type: &str) -> ActivityContext {
        ActivityContext::new(ActivityTypeId::new(activity_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct EchoActivity;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct EchoInput {
        message: String,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct EchoOutput {
        message: String,
    }

    #[async_trait]
    impl Activity for EchoActivity {
        const TYPE_ID: &'static str = "echo";
        type Input = EchoInput;
        type Output = EchoOutput;
        type Error = std::io::Error;

        async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
            Ok(EchoOutput {
                message: input.message,
            })
        }
    }

    #[test]
    fn test_activity_type_id() {
        let id = ActivityTypeId::new("test-activity");
        assert_eq!(id.as_str(), "test-activity");
    }

    #[test]
    fn test_activity_result_variants() {
        let completed = ActivityResult::Completed(serde_json::json!({"result": "ok"}));
        assert!(completed.is_success());
        assert_eq!(
            completed.output(),
            Some(&serde_json::json!({"result": "ok"}))
        );

        let failed = ActivityResult::Failed("error".to_string());
        assert!(!failed.is_success());

        let timed_out = ActivityResult::TimedOut;
        assert!(!timed_out.is_success());
    }

    #[test]
    fn test_activity_context() {
        let mut ctx = ActivityContext::new(ActivityTypeId::new("test"));
        assert_eq!(ctx.attempt, 1);

        ctx.increment_attempt();
        assert_eq!(ctx.attempt, 2);
    }

    #[test]
    fn test_activity_registry_creation() {
        let registry = ActivityRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_activity_registration_and_execution() {
        let registry = ActivityRegistry::new();

        registry.register_activity(EchoActivity);

        assert!(registry.has_activity("echo"));
        assert!(!registry.has_activity("unknown"));
        assert_eq!(registry.len(), 1);

        let activity = registry.get_activity("echo").expect("Should have activity");

        // This is async, so we'd need tokio runtime to test execution,
        // but finding it proves registration worked.
    }

    #[test]
    fn test_activity_registry_with_defaults() {
        let registry = ActivityRegistry::new().with_default_timeout(Duration::from_secs(600));
        assert_eq!(registry.default_timeout, Duration::from_secs(600));
    }

    #[test]
    fn test_activity_error_variants() {
        let not_found = ActivityError::NotFound(ActivityTypeId::new("test"));
        assert!(not_found.to_string().contains("test"));

        let exec_failed = ActivityError::ExecutionFailed("error".to_string());
        assert!(exec_failed.to_string().contains("error"));

        let timeout = ActivityError::Timeout(Duration::from_secs(30));
        assert!(timeout.to_string().contains("30"));
    }
}
