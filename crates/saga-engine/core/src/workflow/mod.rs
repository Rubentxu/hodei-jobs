//!
//! # Workflow Definition and Execution
//!
//! Core traits for defining workflows, steps, and activities in saga-engine.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::event::{EventId, HistoryEvent, SagaId};

/// Unique identifier for a workflow type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkflowTypeId(pub String);

impl WorkflowTypeId {
    pub fn new<T: 'static>() -> Self {
        Self(std::any::type_name::<T>().to_string())
    }

    pub fn from_str(s: &str) -> Self {
        Self(s.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for WorkflowTypeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Configuration for workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub default_timeout_secs: u64,
    pub max_step_retries: u32,
    pub retry_backoff_multiplier: f64,
    pub retry_base_delay_ms: u64,
    pub task_queue: String,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: 3600,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "default".to_string(),
        }
    }
}

/// Context passed during workflow execution
#[derive(Debug, Clone)]
pub struct WorkflowContext {
    pub execution_id: SagaId,
    pub workflow_type: WorkflowTypeId,
    pub current_step_index: usize,
    pub step_outputs: std::collections::HashMap<String, serde_json::Value>,
    pub attempt_number: u32,
    pub config: WorkflowConfig,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl WorkflowContext {
    pub fn new(
        execution_id: SagaId,
        workflow_type: WorkflowTypeId,
        config: WorkflowConfig,
    ) -> Self {
        Self {
            execution_id,
            workflow_type,
            current_step_index: 0,
            step_outputs: std::collections::HashMap::new(),
            attempt_number: 0,
            config,
            started_at: chrono::Utc::now(),
        }
    }

    pub fn set_step_output(&mut self, key: String, value: serde_json::Value) {
        self.step_outputs.insert(key, value);
    }

    pub fn get_step_output(&self, key: &str) -> Option<&serde_json::Value> {
        self.step_outputs.get(key)
    }

    pub fn advance_step(&mut self) {
        self.current_step_index += 1;
        self.attempt_number = 0;
    }

    pub fn increment_attempt(&mut self) {
        self.attempt_number += 1;
    }
}

/// Result of workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowResult {
    Completed {
        output: serde_json::Value,
        steps_executed: usize,
        duration: chrono::Duration,
    },
    Failed {
        error: String,
        failed_step: Option<String>,
        steps_executed: usize,
        compensations_executed: usize,
    },
    Cancelled {
        reason: String,
        steps_executed: usize,
        compensations_executed: usize,
    },
}

/// Input for workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    pub input: serde_json::Value,
    pub idempotency_key: Option<String>,
    pub task_queue: String,
}

/// Activity trait
#[async_trait]
pub trait Activity: Send + Sync + 'static + Debug {
    const TYPE_ID: &'static str;

    type Input: Serialize + for<'de> Deserialize<'de> + Send + Clone;
    type Output: Serialize + for<'de> Deserialize<'de> + Send + Clone;
    type Error: std::error::Error + Send + Sync + 'static;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}

/// Retry policy for activities
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RetryPolicy {
    NoRetry,
    ExponentialBackoff {
        max_retries: u32,
        base_delay_ms: u64,
        multiplier: f64,
        max_delay_ms: u64,
    },
    FixedDelay {
        max_retries: u32,
        delay_ms: u64,
    },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy::ExponentialBackoff {
            max_retries: 3,
            base_delay_ms: 1000,
            multiplier: 2.0,
            max_delay_ms: 60000,
        }
    }
}

/// Workflow step execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepResult {
    Output(serde_json::Value),
    Waiting {
        signal_type: String,
        payload: serde_json::Value,
    },
    Retry {
        error: String,
        delay_ms: u64,
    },
    SideEffect {
        description: String,
    },
}

/// Workflow step error
#[derive(Debug, Serialize, Deserialize)]
pub struct StepError {
    pub error: String,
    pub kind: StepErrorKind,
    pub step_type: String,
    pub is_retryable: bool,
}

impl StepError {
    pub fn new(error: String, kind: StepErrorKind, step_type: String, is_retryable: bool) -> Self {
        Self {
            error,
            kind,
            step_type,
            is_retryable,
        }
    }
}

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Step {} failed: {} ({:?})",
            self.step_type, self.error, self.kind
        )
    }
}

impl std::error::Error for StepError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepErrorKind {
    ActivityFailed,
    ActivityError,
    InvalidInput,
    MissingInput,
    MaxRetriesExceeded,
    Timeout,
    Cancelled,
    Unknown,
}

/// Step compensation error
#[derive(Debug, Serialize, Deserialize)]
pub struct StepCompensationError {
    pub error: String,
    pub step_type: String,
    pub is_retryable: bool,
}

impl StepCompensationError {
    pub fn new(error: String, step_type: String, is_retryable: bool) -> Self {
        Self {
            error,
            step_type,
            is_retryable,
        }
    }
}

impl std::fmt::Display for StepCompensationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Compensation for {} failed: {}",
            self.step_type, self.error
        )
    }
}

impl std::error::Error for StepCompensationError {}

/// Workflow step trait - boxed version for storage
#[async_trait]
pub trait DynWorkflowStep: Send + Sync + 'static {
    fn step_type_id(&self) -> &'static str;
    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError>;
    async fn compensate(
        &self,
        context: &mut WorkflowContext,
        output: serde_json::Value,
    ) -> Result<(), StepCompensationError>;
}

/// Wrapper to implement DynWorkflowStep for any T: WorkflowStep
#[async_trait]
impl<T: WorkflowStep + Debug> DynWorkflowStep for T {
    fn step_type_id(&self) -> &'static str {
        T::TYPE_ID
    }

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        <T as WorkflowStep>::execute(self, context, input).await
    }

    async fn compensate(
        &self,
        context: &mut WorkflowContext,
        output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        <T as WorkflowStep>::compensate(self, context, output).await
    }
}

/// Workflow step base trait
#[async_trait]
pub trait WorkflowStep: Send + Sync + 'static + Debug {
    const TYPE_ID: &'static str;

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError>;

    async fn compensate(
        &self,
        context: &mut WorkflowContext,
        output: serde_json::Value,
    ) -> Result<(), StepCompensationError>;
}

/// Workflow definition trait
pub trait WorkflowDefinition: Send + Sync + 'static {
    const TYPE_ID: &'static str;
    const VERSION: u32;

    fn steps(&self) -> &[Box<dyn DynWorkflowStep>];
    fn configuration(&self) -> WorkflowConfig;

    fn validate_input(&self, _input: &serde_json::Value) -> Result<(), InputValidationError> {
        Ok(())
    }
}

/// Input validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for InputValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Validation error on field '{}': {}",
            self.field, self.message
        )
    }
}

impl std::error::Error for InputValidationError {}

/// Workflow executor
pub struct WorkflowExecutor<W: WorkflowDefinition> {
    workflow: W,
    config: WorkflowConfig,
}

impl<W: WorkflowDefinition> WorkflowExecutor<W> {
    pub fn new(workflow: W) -> Self {
        let config = workflow.configuration();
        Self { workflow, config }
    }

    pub fn workflow(&self) -> &W {
        &self.workflow
    }

    pub fn config(&self) -> &WorkflowConfig {
        &self.config
    }
}

/// Workflow execution error
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowExecutionError {
    pub error: String,
    pub kind: WorkflowExecutionErrorKind,
}

impl std::fmt::Display for WorkflowExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Workflow execution failed: {} ({:?})",
            self.error, self.kind
        )
    }
}

impl std::error::Error for WorkflowExecutionError {}

impl WorkflowExecutionError {
    pub fn new(error: String, kind: WorkflowExecutionErrorKind) -> Self {
        Self { error, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowExecutionErrorKind {
    EventStore,
    InvalidInput,
    StepFailed,
    CompensationFailed,
    Timeout,
    Cancelled,
    Unknown,
}

/// Create workflow started event
pub fn create_workflow_started_event(
    execution_id: &SagaId,
    input: &serde_json::Value,
    idempotency_key: &Option<String>,
) -> HistoryEvent {
    HistoryEvent::new(
        EventId(0),
        execution_id.clone(),
        crate::event::EventType::WorkflowExecutionStarted,
        crate::event::EventCategory::Workflow,
        serde_json::json!({
            "input": input,
            "idempotency_key": idempotency_key,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_type_id() {
        let id = WorkflowTypeId::new::<()>();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn test_workflow_context() {
        let config = WorkflowConfig::default();
        let mut context = WorkflowContext::new(
            SagaId("test-exec".to_string()),
            WorkflowTypeId::from_str("test-workflow"),
            config,
        );

        assert_eq!(context.current_step_index, 0);
        assert_eq!(context.attempt_number, 0);

        context.set_step_output("key1".to_string(), serde_json::json!("value1"));
        assert!(context.get_step_output("key1").is_some());

        context.advance_step();
        assert_eq!(context.current_step_index, 1);
        assert_eq!(context.attempt_number, 0);

        context.increment_attempt();
        assert_eq!(context.attempt_number, 1);
    }

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        match policy {
            RetryPolicy::ExponentialBackoff {
                max_retries,
                base_delay_ms,
                multiplier,
                max_delay_ms,
            } => {
                assert_eq!(max_retries, 3);
                assert_eq!(base_delay_ms, 1000);
                assert_eq!(multiplier, 2.0);
                assert_eq!(max_delay_ms, 60000);
            }
            _ => panic!("Expected ExponentialBackoff"),
        }
    }

    #[test]
    fn test_workflow_started_event() {
        let execution_id = SagaId("test-exec-123".to_string());
        let input = serde_json::json!({"order_id": "ORD-001"});
        let idempotency_key = Some("idem-key-123".to_string());

        let event = create_workflow_started_event(&execution_id, &input, &idempotency_key);

        assert_eq!(event.saga_id, execution_id);
        assert_eq!(
            event.event_type,
            crate::event::EventType::WorkflowExecutionStarted
        );
    }
}

/// Test helpers
pub mod test_helpers {
    use super::*;

    #[derive(Debug, Default)]
    pub struct TestActivity;

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

    #[derive(Debug)]
    pub struct TestStep;

    #[async_trait::async_trait]
    impl WorkflowStep for TestStep {
        const TYPE_ID: &'static str = "test-step";

        async fn execute(
            &self,
            _context: &mut WorkflowContext,
            input: serde_json::Value,
        ) -> Result<StepResult, StepError> {
            Ok(StepResult::Output(input))
        }

        async fn compensate(
            &self,
            _context: &mut WorkflowContext,
            _output: serde_json::Value,
        ) -> Result<(), StepCompensationError> {
            Ok(())
        }
    }
}
