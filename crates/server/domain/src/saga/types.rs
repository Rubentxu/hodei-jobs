//! Saga Types - Core types for the Saga pattern implementation
//!
//! This module defines the fundamental types for orchestrating complex workflows
//! with automatic compensation (rollback) capabilities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ============================================================================
// SagaId - Newtype Pattern for type-safe UUID
// ============================================================================

/// Unique identifier for a saga instance.
///
/// Uses the Newtype Pattern to provide type safety and semantic meaning
/// over a raw UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SagaId(pub Uuid);

impl SagaId {
    /// Creates a new SagaId with a randomly generated UUID
    #[inline]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a SagaId from an existing UUID
    #[inline]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the underlying UUID
    #[inline]
    pub fn into_uuid(self) -> Uuid {
        self.0
    }

    /// Returns a reference to the underlying UUID
    #[inline]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SagaId {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SagaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// SagaType - Classification of saga workflows
// ============================================================================

/// Classification of saga types.
///
/// Each saga type represents a distinct workflow pattern:
/// - **Provisioning**: Worker creation and registration
/// - **Execution**: Job dispatch and execution
/// - **Recovery**: Worker failure recovery and job reassignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaType {
    /// Saga for provisioning workers
    Provisioning,
    /// Saga for executing jobs
    Execution,
    /// Saga for recovering from worker failures
    Recovery,
}

impl SagaType {
    /// Returns the saga type as a string representation
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            SagaType::Provisioning => "provisioning",
            SagaType::Execution => "execution",
            SagaType::Recovery => "recovery",
        }
    }

    /// Returns true if this saga type is provisioning
    #[inline]
    pub fn is_provisioning(&self) -> bool {
        matches!(self, SagaType::Provisioning)
    }

    /// Returns true if this saga type is execution
    #[inline]
    pub fn is_execution(&self) -> bool {
        matches!(self, SagaType::Execution)
    }

    /// Returns true if this saga type is recovery
    #[inline]
    pub fn is_recovery(&self) -> bool {
        matches!(self, SagaType::Recovery)
    }
}

impl fmt::Display for SagaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// SagaState - State machine for saga lifecycle
// ============================================================================

/// State machine for saga lifecycle management.
///
/// Implements the State Pattern to ensure valid state transitions
/// and prevent invalid states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaState {
    /// Initial state, saga has been created but not started
    Pending,
    /// Saga is actively executing steps
    InProgress,
    /// Saga failed and is compensating (rolling back) executed steps
    Compensating,
    /// Saga completed successfully
    Completed,
    /// Saga failed and could not be fully compensated
    Failed,
    /// Saga was cancelled before completion
    Cancelled,
}

impl SagaState {
    /// Checks if a transition to the target state is valid.
    ///
    /// Valid transitions:
    /// - Pending → InProgress
    /// - InProgress → Completed
    /// - InProgress → Compensating (on failure)
    /// - InProgress → Cancelled (explicit cancellation)
    /// - Compensating → Completed (successful compensation)
    /// - Compensating → Failed (compensation failure)
    #[inline]
    pub fn can_transition_to(&self, target: &SagaState) -> bool {
        match self {
            SagaState::Pending => matches!(target, SagaState::InProgress),
            SagaState::InProgress => matches!(
                target,
                SagaState::Completed | SagaState::Compensating | SagaState::Cancelled
            ),
            SagaState::Compensating => matches!(target, SagaState::Completed | SagaState::Failed),
            SagaState::Completed | SagaState::Failed | SagaState::Cancelled => false,
        }
    }

    /// Returns true if the saga is in a terminal state
    #[inline]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SagaState::Completed | SagaState::Failed | SagaState::Cancelled
        )
    }

    /// Returns true if the saga is still active
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SagaState::Pending | SagaState::InProgress | SagaState::Compensating
        )
    }

    /// Returns true if compensation is in progress
    #[inline]
    pub fn is_compensating(&self) -> bool {
        matches!(self, SagaState::Compensating)
    }
}

impl fmt::Display for SagaState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SagaState::Pending => write!(f, "PENDING"),
            SagaState::InProgress => write!(f, "IN_PROGRESS"),
            SagaState::Compensating => write!(f, "COMPENSATING"),
            SagaState::Completed => write!(f, "COMPLETED"),
            SagaState::Failed => write!(f, "FAILED"),
            SagaState::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

// ============================================================================
// SagaError - Error types for saga operations
// ============================================================================

/// Errors that can occur during saga execution.
#[derive(Debug, thiserror::Error)]
pub enum SagaError {
    /// Failed to serialize or deserialize data
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Failed to serialize or deserialize data
    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    /// A saga step failed
    #[error("Step '{step}' failed: {message}")]
    StepFailed {
        step: String,
        message: String,
        will_compensate: bool,
    },

    /// Compensation failed
    #[error("Compensation for step '{step}' failed: {message}")]
    CompensationFailed { step: String, message: String },

    /// Saga timed out
    #[error("Saga timed out after {duration:?}")]
    Timeout { duration: std::time::Duration },

    /// Saga was cancelled
    #[error("Saga was cancelled")]
    Cancelled,

    /// Provider capacity exceeded
    #[error("Provider capacity exceeded: {provider_id}")]
    ProviderCapacityExceeded { provider_id: String },

    /// Infrastructure creation failed
    #[error("Infrastructure creation failed: {message}")]
    InfrastructureCreationFailed { message: String },

    /// Persistence error
    #[error("Persistence error: {message}")]
    PersistenceError { message: String },
}

/// Result type for saga operations.
pub type SagaResult<T = ()> = std::result::Result<T, SagaError>;

// ============================================================================
// SagaStep - Trait for saga steps
// ============================================================================

/// Trait defining a single step in a saga.
///
/// Each step can execute an action and, if needed, compensate (undo) that action.
/// This enables automatic rollback when later steps fail.
#[async_trait::async_trait]
pub trait SagaStep: Send + Sync {
    /// The output type produced by executing this step
    type Output: Send;

    /// Returns the name of this step for logging and debugging
    fn name(&self) -> &'static str;

    /// Executes the step's action.
    ///
    /// Returns Ok(output) on success, or Err on failure.
    /// A failure will trigger compensation of all previously executed steps.
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output>;

    /// Compensates (undoes) the step's action.
    ///
    /// This is called when a later step fails, to restore the system
    /// to its original state before this step executed.
    async fn compensate(&self, output: &Self::Output) -> SagaResult<()>;

    /// Returns true if this step is idempotent.
    ///
    /// An idempotent step can be safely retried without causing
    /// duplicate side effects.
    fn is_idempotent(&self) -> bool {
        false
    }

    /// Returns true if this step has compensation defined.
    fn has_compensation(&self) -> bool {
        true
    }
}

// ============================================================================
// SagaContext - Execution context for saga
// ============================================================================

/// Execution context for a saga instance.
///
/// Carries metadata and state through the saga's execution,
/// including correlation IDs, actor information, and step outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaContext {
    /// Unique identifier for this saga instance
    pub saga_id: SagaId,
    /// Type of saga being executed
    pub saga_type: SagaType,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    /// Actor that initiated the saga
    pub actor: Option<String>,
    /// When the saga started
    pub started_at: DateTime<Utc>,
    /// Current step being executed
    pub current_step: usize,
    /// Whether the saga is currently compensating
    pub is_compensating: bool,
    /// Custom metadata for saga-specific data
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
    /// Step outputs for compensation
    step_outputs: std::collections::HashMap<String, serde_json::Value>,
    /// Error message if saga failed
    pub error_message: Option<String>,
}

impl SagaContext {
    /// Creates a new SagaContext with the given parameters.
    #[inline]
    pub fn new(
        saga_id: SagaId,
        saga_type: SagaType,
        correlation_id: Option<String>,
        actor: Option<String>,
    ) -> Self {
        Self {
            saga_id,
            saga_type,
            correlation_id,
            actor,
            started_at: Utc::now(),
            current_step: 0,
            is_compensating: false,
            metadata: std::collections::HashMap::new(),
            step_outputs: std::collections::HashMap::new(),
            error_message: None,
        }
    }

    /// Creates a new SagaContext from event metadata.
    #[inline]
    pub fn from_event_metadata(
        saga_id: SagaId,
        saga_type: SagaType,
        correlation_id: Option<String>,
        actor: Option<String>,
    ) -> Self {
        Self::new(saga_id, saga_type, correlation_id, actor)
    }

    /// Stores output from a step for later retrieval.
    #[inline]
    pub fn set_step_output<S: Into<String>, V: Serialize>(
        &mut self,
        step_name: S,
        output: &V,
    ) -> SagaResult<()> {
        let value = serde_json::to_value(output)
            .map_err(|e| SagaError::SerializationError(e.to_string()))?;
        self.step_outputs.insert(step_name.into(), value);
        Ok(())
    }

    /// Retrieves output from a previously executed step.
    #[inline]
    pub fn get_step_output<V: for<'de> Deserialize<'de>>(
        &self,
        step_name: &str,
    ) -> Option<SagaResult<V>> {
        self.step_outputs.get(step_name).map(|v| {
            serde_json::from_value(v.clone())
                .map_err(|e| SagaError::DeserializationError(e.to_string()))
        })
    }

    /// Sets an error message when the saga fails.
    #[inline]
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
    }

    /// Returns true if the saga has failed.
    #[inline]
    pub fn has_error(&self) -> bool {
        self.error_message.is_some()
    }

    /// Sets a metadata value.
    #[inline]
    pub fn set_metadata<S: Into<String>, V: Serialize>(
        &mut self,
        key: S,
        value: &V,
    ) -> SagaResult<()> {
        let val = serde_json::to_value(value)
            .map_err(|e| SagaError::SerializationError(e.to_string()))?;
        self.metadata.insert(key.into(), val);
        Ok(())
    }

    /// Gets a metadata value.
    #[inline]
    pub fn get_metadata<V: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<SagaResult<V>> {
        self.metadata.get(key).map(|v| {
            serde_json::from_value(v.clone())
                .map_err(|e| SagaError::DeserializationError(e.to_string()))
        })
    }
}

// ============================================================================
// Saga - Trait representing a complete saga
// ============================================================================

/// Trait representing a complete saga.
///
/// A saga consists of multiple steps that execute sequentially.
/// If any step fails, compensation runs in reverse order.
pub trait Saga: Send + Sync {
    /// Returns the type of this saga
    fn saga_type(&self) -> SagaType;

    /// Returns the steps that make up this saga
    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>>;

    /// Optional: Returns the maximum duration for this saga
    fn timeout(&self) -> Option<std::time::Duration> {
        None
    }

    /// Optional: Returns a unique idempotency key for this saga
    fn idempotency_key(&self) -> Option<&str> {
        None
    }
}

/// Trait for saga orchestrators.
///
/// The orchestrator is responsible for:
/// - Executing saga steps in sequence
/// - Handling compensation on failure
/// - Persisting saga state
/// - Managing concurrency limits
#[async_trait::async_trait]
pub trait SagaOrchestrator: Send + Sync {
    /// Error type for orchestrator operations
    type Error: std::fmt::Debug + Send + Sync;

    /// Executes a saga with the given context.
    ///
    /// # Arguments
    /// * `saga` - The saga to execute
    /// * `context` - The saga context with metadata
    ///
    /// # Returns
    /// * `Result<SagaExecutionResult, Self::Error>` - The result of execution
    async fn execute_saga(
        &self,
        saga: &dyn Saga,
        context: SagaContext,
    ) -> Result<SagaExecutionResult, Self::Error>;

    /// Gets a saga by its ID.
    async fn get_saga(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error>;

    /// Cancels a running saga.
    async fn cancel_saga(&self, saga_id: &SagaId) -> Result<(), Self::Error>;
}

// ============================================================================
// SagaExecutionResult - Result of executing a saga
// ============================================================================

/// Result of executing a saga.
#[derive(Debug, Clone)]
pub struct SagaExecutionResult {
    /// The saga ID
    pub saga_id: SagaId,
    /// The saga type
    pub saga_type: SagaType,
    /// Final state of the saga
    pub state: SagaState,
    /// Number of steps executed successfully
    pub steps_executed: u32,
    /// Number of compensations executed
    pub compensations_executed: u32,
    /// Duration of saga execution
    pub duration: std::time::Duration,
    /// Error message if saga failed
    pub error_message: Option<String>,
}

impl SagaExecutionResult {
    /// Creates a new completed result
    #[inline]
    pub fn completed(saga_id: SagaId, saga_type: SagaType, duration: std::time::Duration) -> Self {
        Self {
            saga_id,
            saga_type,
            state: SagaState::Completed,
            steps_executed: 0,
            compensations_executed: 0,
            duration,
            error_message: None,
        }
    }

    /// Creates a new completed result with step count
    #[inline]
    pub fn completed_with_steps(
        saga_id: SagaId,
        saga_type: SagaType,
        duration: std::time::Duration,
        steps_executed: u32,
    ) -> Self {
        Self {
            saga_id,
            saga_type,
            state: SagaState::Completed,
            steps_executed,
            compensations_executed: 0,
            duration,
            error_message: None,
        }
    }

    /// Creates a new compensated result
    #[inline]
    pub fn compensated(
        saga_id: SagaId,
        saga_type: SagaType,
        duration: std::time::Duration,
        steps_executed: u32,
        compensations_executed: u32,
    ) -> Self {
        Self {
            saga_id,
            saga_type,
            state: SagaState::Compensating,
            steps_executed,
            compensations_executed,
            duration,
            error_message: None,
        }
    }

    /// Creates a new failed result
    #[inline]
    pub fn failed(
        saga_id: SagaId,
        saga_type: SagaType,
        duration: std::time::Duration,
        steps_executed: u32,
        compensations_executed: u32,
        error_message: String,
    ) -> Self {
        Self {
            saga_id,
            saga_type,
            state: SagaState::Failed,
            steps_executed,
            compensations_executed,
            duration,
            error_message: Some(error_message),
        }
    }

    /// Returns true if the saga completed successfully
    #[inline]
    pub fn is_success(&self) -> bool {
        self.state == SagaState::Completed
    }

    /// Returns true if the saga was compensated (rolled back)
    #[inline]
    pub fn is_compensated(&self) -> bool {
        self.state == SagaState::Compensating
    }

    /// Returns true if the saga failed
    #[inline]
    pub fn is_failed(&self) -> bool {
        self.state == SagaState::Failed
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    

    // Test helper struct implementing SagaStep
    struct TestSagaStep;

    #[async_trait]
    impl SagaStep for TestSagaStep {
        type Output = ();

        fn name(&self) -> &'static str {
            "test"
        }

        async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
            Ok(())
        }

        async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
            Ok(())
        }

        fn is_idempotent(&self) -> bool {
            true
        }
    }

    #[test]
    fn saga_id_should_be_valid_uuid() {
        let saga_id = SagaId::new();
        assert!(!saga_id.0.is_nil());
    }

    #[test]
    fn saga_type_should_have_provisioning_variant() {
        assert_eq!(SagaType::Provisioning.as_str(), "provisioning");
    }

    #[test]
    fn saga_type_should_have_execution_variant() {
        assert_eq!(SagaType::Execution.as_str(), "execution");
    }

    #[test]
    fn saga_type_should_have_recovery_variant() {
        assert_eq!(SagaType::Recovery.as_str(), "recovery");
    }

    #[test]
    fn saga_state_transitions_should_be_valid() {
        let mut state = SagaState::Pending;
        assert!(state.can_transition_to(&SagaState::InProgress));
        assert!(!state.can_transition_to(&SagaState::Pending));
        assert!(!state.can_transition_to(&SagaState::Completed));

        state = SagaState::InProgress;
        assert!(state.can_transition_to(&SagaState::Completed));
        assert!(state.can_transition_to(&SagaState::Compensating));
        assert!(state.can_transition_to(&SagaState::Cancelled));
        assert!(!state.can_transition_to(&SagaState::Pending));
    }

    #[test]
    fn saga_step_trait_should_require_execute_method() {
        let step: &dyn SagaStep<Output = ()> = &TestSagaStep;
        assert_eq!(step.name(), "test");
        assert!(step.is_idempotent());
    }

    #[test]
    fn saga_context_should_store_correlation_id() {
        let context = SagaContext::new(
            SagaId::new(),
            SagaType::Provisioning,
            Some("correlation-123".to_string()),
            Some("actor-456".to_string()),
        );
        assert_eq!(context.correlation_id, Some("correlation-123".to_string()));
        assert_eq!(context.actor, Some("actor-456".to_string()));
    }

    #[test]
    fn saga_context_should_store_step_outputs() {
        let mut context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        context.set_step_output("test_step", &"test_value").unwrap();

        let output = context.get_step_output::<String>("test_step").unwrap();
        assert_eq!(output.unwrap(), "test_value");
    }

    #[test]
    fn saga_state_should_identify_terminal_states() {
        assert!(SagaState::Completed.is_terminal());
        assert!(SagaState::Failed.is_terminal());
        assert!(SagaState::Cancelled.is_terminal());
        assert!(!SagaState::Pending.is_terminal());
        assert!(!SagaState::InProgress.is_terminal());
        assert!(!SagaState::Compensating.is_terminal());
    }

    #[test]
    fn saga_state_should_identify_active_states() {
        assert!(SagaState::Pending.is_active());
        assert!(SagaState::InProgress.is_active());
        assert!(SagaState::Compensating.is_active());
        assert!(!SagaState::Completed.is_active());
        assert!(!SagaState::Failed.is_active());
        assert!(!SagaState::Cancelled.is_active());
    }

    #[test]
    fn saga_type_checks_should_work() {
        let provisioning = SagaType::Provisioning;
        let execution = SagaType::Execution;
        let recovery = SagaType::Recovery;

        assert!(provisioning.is_provisioning());
        assert!(!execution.is_provisioning());
        assert!(!recovery.is_provisioning());

        assert!(execution.is_execution());
        assert!(!provisioning.is_execution());
        assert!(!recovery.is_execution());

        assert!(recovery.is_recovery());
        assert!(!provisioning.is_recovery());
        assert!(!execution.is_recovery());
    }

    #[test]
    fn saga_execution_result_should_identify_outcomes() {
        let result = SagaExecutionResult::completed(
            SagaId::new(),
            SagaType::Execution,
            std::time::Duration::from_secs(5),
        );

        assert!(result.is_success());
        assert!(!result.is_compensated());
        assert!(!result.is_failed());
    }

    #[test]
    fn saga_execution_result_should_track_compensation() {
        let result = SagaExecutionResult::compensated(
            SagaId::new(),
            SagaType::Provisioning,
            std::time::Duration::from_secs(3),
            2,
            1,
        );

        assert!(result.is_compensated());
        assert!(!result.is_success());
        assert!(!result.is_failed());
        assert_eq!(result.steps_executed, 2);
        assert_eq!(result.compensations_executed, 1);
    }

    #[test]
    fn saga_execution_result_should_track_failures() {
        let result = SagaExecutionResult::failed(
            SagaId::new(),
            SagaType::Recovery,
            std::time::Duration::from_secs(10),
            3,
            2,
            "Step 'CreateWorker' failed".to_string(),
        );

        assert!(result.is_failed());
        assert!(!result.is_success());
        assert!(!result.is_compensated());
        assert_eq!(result.steps_executed, 3);
        assert_eq!(result.compensations_executed, 2);
        assert!(result.error_message.is_some());
        assert_eq!(result.error_message.unwrap(), "Step 'CreateWorker' failed");
    }
}
