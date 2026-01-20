//!
//! # Workflow Definition and Execution
//!
//! Core traits for defining workflows, steps, and activities in saga-engine.
//! Provides type-safe abstractions for durable workflow execution.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Instant;

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
///
/// This struct tracks all state needed for workflow execution including:
/// - Step progress and outputs
/// - Active signals (for "wait for signal" patterns)
/// - Cancellation state and reasons
/// - Execution metadata for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowContext {
    /// Unique execution identifier
    pub execution_id: SagaId,
    /// Type identifier for this workflow
    pub workflow_type: WorkflowTypeId,
    /// Index of the currently executing or next step
    pub current_step_index: usize,
    /// Outputs from each executed step (type-safe via W::Output)
    pub step_outputs: std::collections::HashMap<String, serde_json::Value>,
    /// Current retry attempt number
    pub attempt_number: u32,
    /// Workflow configuration
    pub config: WorkflowConfig,
    /// When the workflow started
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Active signals waiting for receipt (signal_type -> Vec<payload>])
    pub pending_signals: std::collections::HashMap<String, Vec<serde_json::Value>>,
    /// Cancellation state
    pub cancellation: Option<CancellationState>,
    /// Execution metadata for observability
    pub metadata: WorkflowMetadata,
}

/// State tracking for workflow cancellation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationState {
    /// Reason provided for cancellation
    pub reason: String,
    /// When cancellation was requested
    pub requested_at: chrono::DateTime<chrono::Utc>,
    /// Whether compensation has started
    pub compensating: bool,
    /// Steps already compensated
    pub compensated_steps: usize,
}

/// Metadata for workflow execution observability
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    /// Total number of tasks executed
    pub tasks_executed: u32,
    /// Total number of retries across all steps
    pub total_retries: u32,
    /// Total replay count
    pub replay_count: u32,
    /// Last activity timestamp
    pub last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Custom tags for observability
    pub tags: std::collections::HashMap<String, String>,
}

impl crate::port::replay::Applicator for WorkflowContext {
    fn apply(&mut self, event: &HistoryEvent) -> Result<(), String> {
        self.metadata.last_activity_at = Some(chrono::Utc::now());
        self.metadata.replay_count += 1;

        match event.event_type {
            crate::event::EventType::WorkflowExecutionStarted => {
                // Initialize context state from event attributes
                self.metadata.tasks_executed = 1;
            }
            crate::event::EventType::ActivityTaskCompleted => {
                self.advance_step();
                self.metadata.tasks_executed += 1;
                if let Some(output) = event.attributes.get("output") {
                    self.set_step_output(
                        format!("step_{}", self.current_step_index - 1),
                        output.clone(),
                    );
                }
            }
            crate::event::EventType::ActivityTaskFailed => {
                self.increment_attempt();
                self.metadata.total_retries += 1;
            }
            crate::event::EventType::SignalReceived => {
                if let Some(signal_type) = event.attributes.get("signal_type") {
                    if let Some(payload) = event.attributes.get("payload") {
                        self.add_pending_signal(
                            signal_type.as_str().unwrap_or_default().to_string(),
                            payload.clone(),
                        );
                    }
                }
            }
            crate::event::EventType::WorkflowExecutionCanceled => {
                if let Some(reason) = event.attributes.get("reason") {
                    self.cancellation = Some(CancellationState {
                        reason: reason.as_str().unwrap_or_default().to_string(),
                        requested_at: chrono::Utc::now(),
                        compensating: false,
                        compensated_steps: 0,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn from_snapshot(data: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(data).map_err(|e| e.to_string())
    }
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
            pending_signals: std::collections::HashMap::new(),
            cancellation: None,
            metadata: WorkflowMetadata::default(),
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

    /// Add a pending signal for "wait for signal" patterns
    pub fn add_pending_signal(&mut self, signal_type: String, payload: serde_json::Value) {
        self.pending_signals
            .entry(signal_type)
            .or_insert_with(Vec::new)
            .push(payload);
    }

    /// Consume and return a pending signal of the given type
    pub fn consume_signal(&mut self, signal_type: &str) -> Option<serde_json::Value> {
        self.pending_signals
            .get_mut(signal_type)
            .and_then(|vec| vec.pop())
    }

    /// Check if there are any pending signals of a given type
    pub fn has_pending_signal(&self, signal_type: &str) -> bool {
        self.pending_signals
            .get(signal_type)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Mark the workflow as cancelled
    pub fn mark_cancelled(&mut self, reason: String) {
        self.cancellation = Some(CancellationState {
            reason,
            requested_at: chrono::Utc::now(),
            compensating: false,
            compensated_steps: 0,
        });
    }

    /// Check if the workflow is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_some()
    }

    /// Check if we're in compensation phase
    pub fn is_compensating(&self) -> bool {
        self.cancellation
            .as_ref()
            .map(|s| s.compensating)
            .unwrap_or(false)
    }

    /// Add a tag for observability
    pub fn add_tag(&mut self, key: String, value: String) {
        self.metadata.tags.insert(key, value);
    }

    /// Get total duration so far
    pub fn duration_so_far(&self) -> chrono::Duration {
        chrono::Utc::now().signed_duration_since(self.started_at)
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

/// Represents the possible states of a workflow execution.
///
/// This enum is used to communicate workflow state through the SagaPort
/// and is serializable for external consumption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowState {
    /// Workflow is currently executing
    Running {
        /// Current step being executed
        current_step: Option<u32>,
        /// Elapsed time since start
        elapsed: Option<std::time::Duration>,
    },

    /// Workflow completed successfully
    Completed {
        /// The output produced by the workflow
        output: serde_json::Value,
        /// Total number of steps executed
        steps_executed: u32,
        /// Total duration
        duration: std::time::Duration,
    },

    /// Workflow failed during execution
    Failed {
        /// Error message describing the failure
        error_message: String,
        /// Step where the failure occurred
        failed_step: Option<u32>,
        /// Total number of steps executed before failure
        steps_executed: u32,
        /// Number of compensations executed
        compensations_executed: u32,
    },

    /// Workflow was compensated (rolled back) after partial execution
    Compensated {
        /// Error that triggered compensation
        error_message: String,
        /// Step where the failure occurred
        failed_step: Option<u32>,
        /// Number of steps executed before compensation
        steps_executed: u32,
        /// Number of compensations executed
        compensations_executed: u32,
    },

    /// Workflow was cancelled by user or system
    Cancelled {
        /// Reason for cancellation
        reason: String,
        /// Number of steps executed before cancellation
        steps_executed: u32,
    },

    /// Workflow is currently compensating (rolling back)
    Compensating {
        /// Error that triggered compensation
        error_message: String,
        /// Current compensation step
        current_step: u32,
    },
}

impl WorkflowState {
    /// Check if the workflow is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WorkflowState::Completed { .. }
                | WorkflowState::Failed { .. }
                | WorkflowState::Compensated { .. }
                | WorkflowState::Cancelled { .. }
        )
    }

    /// Check if the workflow completed successfully
    pub fn is_success(&self) -> bool {
        matches!(self, WorkflowState::Completed { .. })
    }

    /// Check if the workflow failed
    pub fn is_failed(&self) -> bool {
        matches!(
            self,
            WorkflowState::Failed { .. } | WorkflowState::Compensated { .. }
        )
    }
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
///
/// This is the core trait for defining workflows in saga-engine v4.0.
/// It provides type-safe input/output handling and step composition.
///
/// # Example
///
/// ```rust,ignore
/// use saga_engine_core::workflow::{WorkflowDefinition, WorkflowConfig, DynWorkflowStep, WorkflowStep, StepResult};
/// use std::sync::Arc;
///
/// struct MyWorkflowInput { value: String }
/// struct MyWorkflowOutput { result: String }
///
/// struct FirstStep;
///
/// #[async_trait::async_trait]
/// impl WorkflowStep for FirstStep {
///     const TYPE_ID: &'static str = "first-step";
///
///     async fn execute(&self, _context: &mut (), input: serde_json::Value) -> Result<StepResult, ()> {
///         Ok(StepResult::Output(input))
///     }
/// }
///
/// struct MyWorkflow;
///
/// impl WorkflowDefinition for MyWorkflow {
///     const TYPE_ID: &'static str = "my-workflow";
///     const VERSION: u32 = 1;
///
///     type Input = MyWorkflowInput;
///     type Output = MyWorkflowOutput;
///
///     fn steps(&self) -> &[Box<dyn DynWorkflowStep>] {
///         vec![]
///     }
///
///     fn configuration(&self) -> WorkflowConfig {
///         WorkflowConfig::default()
///     }
/// }
/// ```
pub trait WorkflowDefinition: Send + Sync + 'static {
    /// Unique identifier for this workflow type
    const TYPE_ID: &'static str;
    /// Version number for this workflow definition
    const VERSION: u32;

    /// Type-safe input for this workflow
    type Input: Serialize + for<'de> Deserialize<'de> + Send + Clone + Debug;
    /// Type-safe output for this workflow
    type Output: Serialize + for<'de> Deserialize<'de> + Send + Clone + Debug;

    /// Returns the steps that compose this workflow
    fn steps(&self) -> &[Box<dyn DynWorkflowStep>];
    /// Returns the workflow configuration
    fn configuration(&self) -> WorkflowConfig;

    /// Validates the input before workflow execution begins
    ///
    /// Override this method to add custom validation logic.
    /// By default, all inputs are accepted.
    fn validate_input(&self, _input: &Self::Input) -> Result<(), InputValidationError> {
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

/// Workflow runtime trait for saga-engine v4.0
#[async_trait]
pub trait WorkflowRuntime: Send + Sync + Debug {
    /// Start a new workflow
    async fn start_workflow(
        &self,
        input: serde_json::Value,
        idempotency_key: Option<String>,
    ) -> Result<SagaId, WorkflowExecutionError>;

    /// Get workflow status
    async fn get_workflow_status(
        &self,
        execution_id: &SagaId,
    ) -> Result<Option<WorkflowResult>, WorkflowExecutionError>;
}

/// Metrics trait for workflow observability
///
/// Implement this trait to collect metrics from the workflow executor.
/// Typical implementations would record to Prometheus, StatsD, or OTLP.
pub trait WorkflowMetrics: Send + Sync + Debug + 'static {
    /// Record a workflow starting
    fn record_workflow_start(&self, workflow_type: &str);
    /// Record a workflow completing successfully
    fn record_workflow_complete(&self, workflow_type: &str, duration_ms: u64);
    /// Record a workflow failing
    fn record_workflow_failure(&self, workflow_type: &str, error_type: &str);
    /// Record a workflow error (non-failure, e.g., event store error)
    fn record_workflow_error(&self, workflow_type: &str, error_location: &str);
    /// Record workflow initialization duration
    fn record_workflow_init_duration(&self, workflow_type: &str, duration_ms: u64);
    /// Record a step completing successfully
    fn record_step_success(&self, step_type: &str, duration_ms: u64);
    /// Record a step failing
    fn record_step_failure(&self, step_type: &str, duration_ms: u64);
    /// Record a retry
    fn record_retry(&self, step_type: &str, delay_ms: u64);
    /// Record replay duration
    fn record_replay_duration(&self, workflow_type: &str, duration_ms: u64);
    /// Record snapshot frequency
    fn record_snapshot(&self, workflow_type: &str, events_count: u64);
}

/// No-op metrics implementation for testing or when metrics aren't needed
#[derive(Debug, Default, Clone)]
pub struct NoopMetrics;

impl WorkflowMetrics for NoopMetrics {
    fn record_workflow_start(&self, _workflow_type: &str) {}
    fn record_workflow_complete(&self, _workflow_type: &str, _duration_ms: u64) {}
    fn record_workflow_failure(&self, _workflow_type: &str, _error_type: &str) {}
    fn record_workflow_error(&self, _workflow_type: &str, _error_location: &str) {}
    fn record_workflow_init_duration(&self, _workflow_type: &str, _duration_ms: u64) {}
    fn record_step_success(&self, _step_type: &str, _duration_ms: u64) {}
    fn record_step_failure(&self, _step_type: &str, _duration_ms: u64) {}
    fn record_retry(&self, _step_type: &str, _delay_ms: u64) {}
    fn record_replay_duration(&self, _workflow_type: &str, _duration_ms: u64) {}
    fn record_snapshot(&self, _workflow_type: &str, _events_count: u64) {}
}

/// A durable implementation of WorkflowRuntime
///
/// This runtime provides fault-tolerant workflow execution by combining:
/// - JetStream-based task queue for reliable task delivery
/// - PostgreSQL event sourcing for state reconstruction
/// - Automatic retry with configurable backoff
pub struct DurableWorkflowRuntime<W, Q, E, R>
where
    W: WorkflowDefinition,
    Q: crate::port::TaskQueue,
    E: crate::port::EventStore,
    R: crate::port::HistoryReplayer<WorkflowContext>,
{
    workflow: W,
    task_queue: Arc<Q>,
    event_store: Arc<E>,
    replayer: Arc<R>,
    /// Metrics recorder for observability
    metrics: Arc<dyn WorkflowMetrics + Send + Sync>,
}

impl<W, Q, E, R> Debug for DurableWorkflowRuntime<W, Q, E, R>
where
    W: WorkflowDefinition,
    Q: crate::port::TaskQueue,
    E: crate::port::EventStore,
    R: crate::port::HistoryReplayer<WorkflowContext>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DurableWorkflowRuntime")
            .field("workflow", &W::TYPE_ID)
            .field("version", &W::VERSION)
            .finish_non_exhaustive()
    }
}

impl<W, Q, E, R> DurableWorkflowRuntime<W, Q, E, R>
where
    W: WorkflowDefinition,
    Q: crate::port::TaskQueue,
    E: crate::port::EventStore,
    R: crate::port::HistoryReplayer<WorkflowContext>,
{
    /// Create a new durable workflow runtime
    pub fn new(
        workflow: W,
        task_queue: Arc<Q>,
        event_store: Arc<E>,
        replayer: Arc<R>,
        metrics: Arc<dyn WorkflowMetrics + Send + Sync>,
    ) -> Self {
        Self {
            workflow,
            task_queue,
            event_store,
            replayer,
            metrics,
        }
    }

    /// Execute a single step with retry logic
    async fn execute_step(
        &self,
        context: &mut WorkflowContext,
        step: &dyn DynWorkflowStep,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let step_start = Instant::now();
        let step_type = step.step_type_id();
        let attempt = context.attempt_number;

        // Check if we should retry
        if attempt >= self.workflow.configuration().max_step_retries {
            return Err(StepError::new(
                format!(
                    "Max retries ({}) exceeded",
                    self.workflow.configuration().max_step_retries
                ),
                StepErrorKind::MaxRetriesExceeded,
                step_type.to_string(),
                false,
            ));
        }

        // Execute with tracing
        let _span = tracing::info_span!("step_execution", step_type = step_type);
        let _guard = _span.enter();
        let result = step.execute(context, input).await;

        // Record metrics
        let duration_ms = step_start.elapsed().as_millis() as u64;
        match &result {
            Ok(_) => {
                self.metrics.record_step_success(step_type, duration_ms);
            }
            Err(err) => {
                self.metrics.record_step_failure(step_type, duration_ms);
                if err.is_retryable {
                    // Calculate backoff with jitter
                    let delay = self.calculate_retry_delay(attempt);
                    self.metrics.record_retry(step_type, delay);
                    return Ok(StepResult::Retry {
                        error: err.error.clone(),
                        delay_ms: delay,
                    });
                }
            }
        }

        result
    }

    /// Calculate retry delay with exponential backoff and jitter
    fn calculate_retry_delay(&self, attempt: u32) -> u64 {
        let config = self.workflow.configuration();
        let base_delay = config.retry_base_delay_ms;
        let multiplier = config.retry_backoff_multiplier;
        let max_delay = 60000; // 60 seconds max

        let delay = (base_delay as f64 * multiplier.powf(attempt as f64)) as u64;
        let jitter = (rand::random::<f64>() * 0.2 + 0.9) as u64; // 10-30% jitter
        (delay * jitter / 100).min(max_delay)
    }
}

#[async_trait]
impl<W, Q, E, R> WorkflowRuntime for DurableWorkflowRuntime<W, Q, E, R>
where
    W: WorkflowDefinition,
    Q: crate::port::TaskQueue + 'static,
    E: crate::port::EventStore + 'static,
    R: crate::port::HistoryReplayer<WorkflowContext, Error = E::Error> + 'static,
{
    async fn start_workflow(
        &self,
        input: serde_json::Value,
        idempotency_key: Option<String>,
    ) -> Result<SagaId, WorkflowExecutionError> {
        let execution_id = SagaId::new();
        let start_time = Instant::now();

        tracing::info!(
            execution_id = %execution_id.0,
            workflow_type = W::TYPE_ID,
            "Starting workflow execution"
        );

        // Record workflow start metric
        self.metrics.record_workflow_start(W::TYPE_ID);

        // 1. Create start event
        let event = create_workflow_started_event(&execution_id, &input, &idempotency_key);

        // 2. Persist event
        self.event_store
            .append_event(&execution_id, 0, &event)
            .await
            .map_err(|e| {
                self.metrics
                    .record_workflow_error(W::TYPE_ID, "event_store");
                WorkflowExecutionError::new(
                    format!("Failed to persist start event: {:?}", e),
                    WorkflowExecutionErrorKind::EventStore,
                )
            })?;

        // 3. Queue first task
        let payload = serde_json::json!({
            "step_index": 0,
        });
        let task = crate::port::task_queue::Task::new(
            "workflow-step".to_string(),
            execution_id.clone(),
            execution_id.0.clone(),
            serde_json::to_vec(&payload).map_err(|e| {
                WorkflowExecutionError::new(
                    format!("Failed to serialize task payload: {}", e),
                    WorkflowExecutionErrorKind::Unknown,
                )
            })?,
        )
        .with_queue(self.workflow.configuration().task_queue.clone());

        self.task_queue
            .publish(&task, &self.workflow.configuration().task_queue)
            .await
            .map_err(|e| {
                self.metrics.record_workflow_error(W::TYPE_ID, "task_queue");
                WorkflowExecutionError::new(
                    format!("Failed to queue initial task: {:?}", e),
                    WorkflowExecutionErrorKind::Unknown,
                )
            })?;

        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.metrics
            .record_workflow_init_duration(W::TYPE_ID, duration_ms);

        Ok(execution_id)
    }

    async fn get_workflow_status(
        &self,
        execution_id: &SagaId,
    ) -> Result<Option<WorkflowResult>, WorkflowExecutionError> {
        // Replay and check status
        let replay_start = Instant::now();
        let replay_result = self
            .replayer
            .get_current_state(execution_id, None)
            .await
            .map_err(|e| {
                WorkflowExecutionError::new(
                    format!("Failed to replay state: {:?}", e),
                    WorkflowExecutionErrorKind::Unknown,
                )
            })?;

        let replay_ms = replay_start.elapsed().as_millis() as u64;
        self.metrics.record_replay_duration(W::TYPE_ID, replay_ms);

        // Determine workflow result from context
        let context = &replay_result.state;
        if context.is_cancelled() {
            Ok(Some(WorkflowResult::Cancelled {
                reason: context
                    .cancellation
                    .as_ref()
                    .map(|c| c.reason.clone())
                    .unwrap_or_default(),
                steps_executed: context.current_step_index,
                compensations_executed: context
                    .cancellation
                    .as_ref()
                    .map(|c| c.compensated_steps)
                    .unwrap_or(0),
            }))
        } else {
            // Return None if workflow is still running
            Ok(None)
        }
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
