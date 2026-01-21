//! Durable Workflow implementation for Workflow-as-Code pattern.
//!
//! This module provides the [`DurableWorkflow`] trait which allows defining
//! workflows as regular Rust code with `async fn run()` instead of declarative
//! step lists. This is the main innovation for saga-engine v4.0.
//!
//! # Example
//!
//! ```rust
//! use saga_engine_core::workflow::{DurableWorkflow, WorkflowContext};
//!
//! struct MyWorkflow;
//!
//! #[async_trait::async_trait]
//! impl DurableWorkflow for MyWorkflow {
//!     const TYPE_ID: &'static str = "my-workflow";
//!     const VERSION: u32 = 1;
//!
//!     type Input = serde_json::Value;
//!     type Output = serde_json::Value;
//!     type Error = std::io::Error;
//!
//!     async fn run(
//!         &self,
//!         _ctx: &mut WorkflowContext,
//!         input: Self::Input,
//!     ) -> Result<Self::Output, Self::Error> {
//!         // Code is Rust - if, match, loops, error handling all work!
//!         // Use ctx.execute_activity() for activities (see execute_activity method)
//!         Ok(input)
//!     }
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

use crate::event::SagaId;
use crate::workflow::WorkflowContext;

/// Trait for workflows defined as Rust code (Workflow-as-Code).
///
/// This is the main trait for saga-engine v4.0. Unlike [`WorkflowDefinition`]
/// which uses a declarative step list, `DurableWorkflow` uses an imperative
/// `run()` method that is actual Rust code.
///
/// # Key Differences from WorkflowDefinition
///
/// | Aspect | WorkflowDefinition | DurableWorkflow |
/// |--------|-------------------|-----------------|
/// | Definition | Step list | `async fn run()` |
/// | Logic | Declarative | Imperative |
/// | Control Flow | Limited | Full Rust control flow |
/// | Type Safety | Via generics | Via generics |
/// | Replay | Via step boundaries | Via activity completion |
///
/// # Execution Model
///
/// 1. Engine calls `run(ctx, input)`
/// 2. User code calls `ctx.execute_activity()` for long-running operations
/// 3. Engine detects pause signal and persists state
/// 4. When activity completes, engine calls `run()` again from where it paused
/// 5. Repeat until `run()` returns `Ok(output)` or `Err(error)`
///
/// This model enables "deterministic replay" - the same input always produces
/// the same output regardless of when activities complete.
#[async_trait]
pub trait DurableWorkflow: Send + Sync + 'static {
    /// Unique identifier for this workflow type.
    const TYPE_ID: &'static str;

    /// Version number for this workflow definition.
    ///
    /// Incrementing this version allows the engine to detect when workflow
    /// definitions have changed. This is important for replay correctness.
    const VERSION: u32 = 1;

    /// Type-safe input for this workflow.
    ///
    /// Must be serializable for persistence and replay.
    type Input: Serialize + for<'de> Deserialize<'de> + Send + Clone + Debug;

    /// Type-safe output for this workflow.
    ///
    /// Must be serializable for persistence and result retrieval.
    type Output: Serialize + for<'de> Deserialize<'de> + Send + Clone + Debug;

    /// Error type for this workflow.
    ///
    /// All errors from this workflow must be this type.
    /// The engine will wrap infrastructure errors appropriately.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Main workflow method - called by the engine for execution.
    ///
    /// This method should contain all the workflow logic. It calls
    /// `ctx.execute_activity()` for any long-running operations.
    ///
    /// # Arguments
    ///
    /// * `context` - The workflow context providing state and operations.
    /// * `input` - The workflow input, type-safe via `Self::Input`.
    ///
    /// # Returns
    ///
    /// * `Ok(output)` - Workflow completed successfully with result.
    /// * `Err(error)` - Workflow failed with error.
    ///
    /// # Pausing
    ///
    /// The method should NOT explicitly pause. Instead, call
    /// `ctx.execute_activity()` which returns `Err(ExecuteActivityError::Paused)`
    /// when the activity is not yet complete. The engine handles the pause
    /// automatically.
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error>;
}

/// Error thrown when a workflow needs to pause waiting for an activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPaused {
    /// The activity type that was scheduled.
    pub activity_type: &'static str,
    /// The saga/execution ID.
    pub execution_id: SagaId,
    /// The activity ID for tracking.
    pub activity_id: String,
    /// The input for the activity.
    pub input: serde_json::Value,
}

impl std::fmt::Display for WorkflowPaused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Workflow paused waiting for {} (execution: {}, activity: {})",
            self.activity_type, self.execution_id.0, self.activity_id
        )
    }
}

impl std::error::Error for WorkflowPaused {}

/// Error types for activity execution in durable workflows.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ExecuteActivityError<E: std::error::Error + Send + Sync + 'static> {
    inner: ExecuteActivityErrorInner<E>,
}

#[derive(Debug, thiserror::Error)]
enum ExecuteActivityErrorInner<E> {
    #[error("Activity completed successfully")]
    Ok(serde_json::Value),

    #[error("Workflow paused waiting for activity")]
    Paused(WorkflowPaused),

    #[error("Activity failed: {0}")]
    Failed(Arc<E>),

    #[error("Activity timeout: {0:?}")]
    Timeout(std::time::Duration),

    #[error("Activity cancelled")]
    Cancelled,

    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl<E> ExecuteActivityError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    /// Create Ok variant.
    pub fn ok(value: serde_json::Value) -> Self {
        Self {
            inner: ExecuteActivityErrorInner::Ok(value),
        }
    }

    /// Create Paused variant.
    pub fn paused(paused: WorkflowPaused) -> Self {
        Self {
            inner: ExecuteActivityErrorInner::Paused(paused),
        }
    }

    /// Create Failed variant.
    pub fn failed(error: E) -> Self {
        Self {
            inner: ExecuteActivityErrorInner::Failed(Arc::new(error)),
        }
    }

    /// Create Timeout variant.
    pub fn timeout(duration: std::time::Duration) -> Self {
        Self {
            inner: ExecuteActivityErrorInner::Timeout(duration),
        }
    }

    /// Create Cancelled variant.
    pub fn cancelled() -> Self {
        Self {
            inner: ExecuteActivityErrorInner::Cancelled,
        }
    }

    /// Create Serialization variant.
    pub fn serialization(error: String) -> Self {
        Self {
            inner: ExecuteActivityErrorInner::Serialization(error),
        }
    }

    /// Check if this is a pause signal (normal flow).
    pub fn is_paused(&self) -> bool {
        matches!(self.inner, ExecuteActivityErrorInner::Paused(_))
    }

    /// Check if this is an Ok variant.
    pub fn is_ok(&self) -> bool {
        matches!(self.inner, ExecuteActivityErrorInner::Ok(_))
    }

    /// Check if this is a permanent failure.
    pub fn is_failed(&self) -> bool {
        matches!(self.inner, ExecuteActivityErrorInner::Failed(_))
    }

    /// Get the inner error if this is a Failed variant.
    pub fn as_failed(&self) -> Option<&E> {
        match &self.inner {
            ExecuteActivityErrorInner::Failed(e) => Some(e.as_ref()),
            _ => None,
        }
    }

    /// Get the paused info if this is a Paused variant.
    pub fn as_paused(&self) -> Option<&WorkflowPaused> {
        match &self.inner {
            ExecuteActivityErrorInner::Paused(p) => Some(p),
            _ => None,
        }
    }

    /// Get the ok value if this is an Ok variant.
    pub fn as_ok(&self) -> Option<&serde_json::Value> {
        match &self.inner {
            ExecuteActivityErrorInner::Ok(v) => Some(v),
            _ => None,
        }
    }

    /// Get the timeout if this is a Timeout variant.
    pub fn as_timeout(&self) -> Option<&std::time::Duration> {
        match &self.inner {
            ExecuteActivityErrorInner::Timeout(d) => Some(d),
            _ => None,
        }
    }

    /// Get the serialization error if this is a Serialization variant.
    pub fn as_serialization(&self) -> Option<&String> {
        match &self.inner {
            ExecuteActivityErrorInner::Serialization(e) => Some(e),
            _ => None,
        }
    }
}

/// Activity execution options for fine-grained control.
#[derive(Debug, Clone)]
pub struct ActivityOptions {
    /// Timeout for the entire activity execution.
    pub timeout: std::time::Duration,
    /// Retry policy for transient failures.
    pub retry_policy: crate::workflow::RetryPolicy,
    /// Heartbeat interval for long-running activities.
    pub heartbeat_interval: Option<std::time::Duration>,
}

impl Default for ActivityOptions {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(300), // 5 minutes
            retry_policy: crate::workflow::RetryPolicy::default(),
            heartbeat_interval: None,
        }
    }
}

impl ActivityOptions {
    /// Create with a specific timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Create with a specific retry policy.
    pub fn with_retry(mut self, policy: crate::workflow::RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Create with a heartbeat interval.
    pub fn with_heartbeat(mut self, interval: std::time::Duration) -> Self {
        self.heartbeat_interval = Some(interval);
        self
    }
}

/// Workflow execution state for durable workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DurableWorkflowState {
    /// Workflow is running.
    Running,
    /// Workflow is paused waiting for activity.
    Paused {
        /// The activity being waited for.
        waiting_for: &'static str,
        /// The activity ID.
        activity_id: String,
    },
    /// Workflow completed successfully.
    Completed,
    /// Workflow failed.
    Failed {
        /// Error message.
        error: String,
        /// The step where it failed.
        step: Option<String>,
    },
    /// Workflow was cancelled.
    Cancelled {
        /// Cancellation reason.
        reason: String,
    },
}

impl DurableWorkflowState {
    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DurableWorkflowState::Completed
                | DurableWorkflowState::Failed { .. }
                | DurableWorkflowState::Cancelled { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::SagaId;
    use crate::workflow::{Activity, WorkflowConfig};

    // Test activity
    #[derive(Debug)]
    struct TestActivity;

    #[async_trait::async_trait]
    impl Activity for TestActivity {
        const TYPE_ID: &'static str = "test-activity";

        type Input = serde_json::Value;
        type Output = serde_json::Value;
        type Error = std::io::Error;

        async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
            Ok(input)
        }
    }

    // Test durable workflow
    #[derive(Debug)]
    struct TestDurableWorkflow;

    #[async_trait::async_trait]
    impl DurableWorkflow for TestDurableWorkflow {
        const TYPE_ID: &'static str = "test-durable";
        const VERSION: u32 = 1;

        type Input = serde_json::Value;
        type Output = serde_json::Value;
        type Error = std::io::Error;

        async fn run(
            &self,
            _ctx: &mut WorkflowContext,
            input: Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(input)
        }
    }

    #[test]
    fn test_durable_workflow_type_id() {
        assert_eq!(TestDurableWorkflow::TYPE_ID, "test-durable");
        assert_eq!(TestDurableWorkflow::VERSION, 1);
    }

    #[test]
    fn test_workflow_paused_display() {
        let paused = WorkflowPaused {
            activity_type: "test-activity",
            execution_id: SagaId("test-123".to_string()),
            activity_id: "act-456".to_string(),
            input: serde_json::json!({}),
        };
        let display = format!("{}", paused);
        assert!(display.contains("test-activity"));
        assert!(display.contains("test-123"));
        assert!(display.contains("act-456"));
    }

    #[test]
    fn test_execute_activity_error_variants() {
        let ok: ExecuteActivityError<std::io::Error> =
            ExecuteActivityError::ok(serde_json::json!("result"));
        assert!(ok.is_ok() && !ok.is_paused());

        let paused: ExecuteActivityError<std::io::Error> =
            ExecuteActivityError::paused(WorkflowPaused {
                activity_type: "test",
                execution_id: SagaId("test".to_string()),
                activity_id: "act".to_string(),
                input: serde_json::json!({}),
            });
        assert!(paused.is_paused());
        assert!(!paused.is_failed());
    }

    #[test]
    fn test_activity_options_defaults() {
        let opts = ActivityOptions::default();
        assert_eq!(opts.timeout, std::time::Duration::from_secs(300));
        assert!(opts.heartbeat_interval.is_none());
    }

    #[test]
    fn test_activity_options_builder() {
        let opts = ActivityOptions::default()
            .with_timeout(std::time::Duration::from_secs(600))
            .with_heartbeat(std::time::Duration::from_secs(30));

        assert_eq!(opts.timeout, std::time::Duration::from_secs(600));
        assert_eq!(
            opts.heartbeat_interval,
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[test]
    fn test_durable_workflow_state() {
        let running = DurableWorkflowState::Running;
        assert!(!running.is_terminal());

        let paused = DurableWorkflowState::Paused {
            waiting_for: "test",
            activity_id: "act-1".to_string(),
        };
        assert!(!paused.is_terminal());

        let completed = DurableWorkflowState::Completed;
        assert!(completed.is_terminal());

        let failed = DurableWorkflowState::Failed {
            error: "test error".to_string(),
            step: Some("step-1".to_string()),
        };
        assert!(failed.is_terminal());

        let cancelled = DurableWorkflowState::Cancelled {
            reason: "user request".to_string(),
        };
        assert!(cancelled.is_terminal());
    }
}
