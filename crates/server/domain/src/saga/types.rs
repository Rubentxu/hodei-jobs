//! Saga Types - Core types for the Saga pattern implementation
//!
//! This module defines the fundamental types for orchestrating complex workflows
//! with automatic compensation (rollback) capabilities.

// ============================================================================
// Imports
// ============================================================================

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Idempotency - saga_id generation with UUID v5
// ============================================================================

/// Generates a deterministic saga ID for execution based on job ID.
///
/// This ensures idempotency: the same job will always produce the same
/// saga ID, preventing duplicate saga creation when NATS delivers
/// the same message multiple times.
///
/// # Arguments
/// * `job_id` - The job ID to generate the saga ID for
///
/// # Returns
/// A deterministic UUID v5 based on the job ID
///
/// # Example
/// ```rust
/// use hodei_server_domain::saga::saga_id_for_job;
///
/// let job_id = "job-123";
/// let saga_id = saga_id_for_job(job_id);
/// // Same job_id always produces the same saga_id
/// assert_eq!(saga_id, saga_id_for_job(job_id));
/// ```
#[inline]
pub fn saga_id_for_job(job_id: &str) -> Uuid {
    let namespace = Uuid::NAMESPACE_OID;
    let input = format!("execution-saga-{}", job_id);
    Uuid::new_v5(&namespace, input.as_bytes())
}

/// Generates a deterministic saga ID for provisioning based on provider and job.
///
/// # Arguments
/// * `provider_id` - The provider identifier
/// * `job_id` - The job ID
///
/// # Returns
/// A deterministic UUID v5 based on provider and job
#[inline]
pub fn saga_id_for_provisioning(provider_id: &str, job_id: &str) -> Uuid {
    let namespace = Uuid::NAMESPACE_OID;
    let input = format!("provisioning-saga-{}:{}", provider_id, job_id);
    Uuid::new_v5(&namespace, input.as_bytes())
}

/// Generates a deterministic saga ID for recovery based on job ID.
///
/// # Arguments
/// * `job_id` - The job ID
///
/// # Returns
/// A deterministic UUID v5 based on job
#[inline]
pub fn saga_id_for_recovery(job_id: &str) -> Uuid {
    let namespace = Uuid::NAMESPACE_OID;
    let input = format!("recovery-saga-{}", job_id);
    Uuid::new_v5(&namespace, input.as_bytes())
}

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

    /// Creates a deterministic SagaId from a string (for idempotency)
    /// 
    /// Uses UUID v5 (SHA-1 namespace) to generate reproducible IDs.
    /// Same input always produces the same SagaId.
    /// 
    /// # Example
    /// ```
    /// let saga_id = SagaId::from_string("execution-job123-worker456");
    /// ```
    #[inline]
    pub fn from_string(s: &str) -> Self {
        // Use DNS namespace as base (standard practice)
        let namespace = Uuid::NAMESPACE_DNS;
        Self(Uuid::new_v5(&namespace, s.as_bytes()))
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
/// - **Cancellation**: Job cancellation workflow (EPIC-46 GAP-07)
/// - **Timeout**: Job timeout handling (EPIC-46 GAP-08)
/// - **Cleanup**: Resource cleanup and maintenance (EPIC-46 GAP-09)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaType {
    /// Saga for provisioning workers
    Provisioning,
    /// Saga for executing jobs
    Execution,
    /// Saga for recovering from worker failures
    Recovery,
    /// Saga for cancelling jobs (EPIC-46 GAP-07)
    Cancellation,
    /// Saga for handling job timeouts (EPIC-46 GAP-08)
    Timeout,
    /// Saga for resource cleanup (EPIC-46 GAP-09)
    Cleanup,
}

impl SagaType {
    /// Returns the saga type as a string representation
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            SagaType::Provisioning => "PROVISIONING",
            SagaType::Execution => "EXECUTION",
            SagaType::Recovery => "RECOVERY",
            SagaType::Cancellation => "CANCELLATION",
            SagaType::Timeout => "TIMEOUT",
            SagaType::Cleanup => "CLEANUP",
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

    /// Returns true if this saga type is cancellation (EPIC-46 GAP-07)
    #[inline]
    pub fn is_cancellation(&self) -> bool {
        matches!(self, SagaType::Cancellation)
    }

    /// Returns true if this saga type is timeout (EPIC-46 GAP-08)
    #[inline]
    pub fn is_timeout(&self) -> bool {
        matches!(self, SagaType::Timeout)
    }

    /// Returns true if this saga type is cleanup (EPIC-46 GAP-09)
    #[inline]
    pub fn is_cleanup(&self) -> bool {
        matches!(self, SagaType::Cleanup)
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
    ///
    /// # Arguments
    /// * `context` - The saga context containing metadata from the execute phase.
    ///   Steps should use `context.get_metadata()` to retrieve data stored
    ///   during `execute()` for performing compensation actions.
    ///
    /// # Design Change (EPIC-SAGA-ENGINE)
    /// The signature changed from `compensate(&self, output: &Self::Output)` to
    /// `compensate(&self, context: &mut SagaContext)` to enable compensation
    /// logic to access metadata stored during execute phase. This allows steps
    /// to perform real compensation actions (e.g., destroying a worker) rather
    /// than relying on output stored in a separate field.
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()>;

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
/// EPIC-45 Gap 6: Added state field for reactive processor
/// EPIC-46 GAP-02: Added version for Optimistic Locking and trace_parent for distributed tracing
#[derive(Clone)]
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
    /// Runtime services injected for step execution (not persisted)
    #[doc(hidden)]
    pub services: Option<Arc<SagaServices>>,
    /// EPIC-45 Gap 6: Current saga state
    pub state: SagaState,
    /// EPIC-46 GAP-02: Version for Optimistic Locking concurrency control
    pub version: u64,
    /// EPIC-46 GAP-14: W3C Trace Context for distributed tracing propagation
    pub trace_parent: Option<String>,
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
            services: None,
            state: SagaState::Pending,
            version: 0,
            trace_parent: None,
        }
    }

    /// Creates a fully initialized SagaContext from persisted data.
    /// EPIC-45 Gap 6: Added state parameter
    /// EPIC-46 GAP-02: Added version parameter
    /// EPIC-46 GAP-14: Added trace_parent parameter
    #[inline]
    pub fn from_persistence(
        saga_id: SagaId,
        saga_type: SagaType,
        correlation_id: Option<String>,
        actor: Option<String>,
        started_at: DateTime<Utc>,
        current_step: usize,
        is_compensating: bool,
        metadata: std::collections::HashMap<String, serde_json::Value>,
        error_message: Option<String>,
        state: SagaState,
        version: u64,
        trace_parent: Option<String>,
    ) -> Self {
        Self {
            saga_id,
            saga_type,
            correlation_id,
            actor,
            started_at,
            current_step,
            is_compensating,
            metadata,
            step_outputs: std::collections::HashMap::new(),
            error_message,
            services: None,
            state,
            version,
            trace_parent,
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

    /// Injects runtime services for step execution.
    /// These services are not persisted and are only available during execution.
    #[inline]
    pub fn with_services(self, services: Arc<SagaServices>) -> Self {
        Self {
            services: Some(services),
            ..self
        }
    }

    /// Gets the injected services, if available.
    #[inline]
    pub fn services(&self) -> Option<&Arc<SagaServices>> {
        self.services.as_ref()
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

/// Container for saga runtime services.
///
/// This structure holds references to the services needed by saga steps
/// during execution. These services are injected at runtime and are not
/// persisted with the saga context.
///
/// EPIC-46 GAP-20: Extended with additional services for complete saga support.
/// EPIC-50: Added CommandBus for saga command dispatch (Type Erasure pattern)
pub struct SagaServices {
    /// Provider registry for infrastructure operations
    pub provider_registry: Arc<dyn crate::workers::WorkerRegistry + Send + Sync>,
    /// Event bus for publishing domain events
    pub event_bus: Arc<dyn crate::event_bus::EventBus + Send + Sync>,
    /// Job repository for job operations
    pub job_repository: Option<Arc<dyn crate::jobs::JobRepository + Send + Sync>>,
    /// Worker provisioning service for creating/destroying workers (EPIC-SAGA-ENGINE)
    ///
    /// This field is optional because not all sagas require worker provisioning.
    /// Provisioning sagas will inject this service to enable real infrastructure
    /// creation within saga steps.
    pub provisioning_service: Option<Arc<dyn crate::workers::WorkerProvisioning + Send + Sync>>,
    /// EPIC-46 GAP-20: Saga orchestrator for nested saga execution
    pub orchestrator: Option<Arc<dyn SagaOrchestrator<Error = crate::saga::OrchestratorError> + Send + Sync>>,
    /// EPIC-50: Type-erased CommandBus for dispatching commands from saga steps
    ///
    /// Uses the ErasedCommandBus trait with Arc<dyn ErasedCommandBus> to avoid
    /// the E0038 error (trait not dyn compatible due to generic methods).
    /// Commands are dispatched using the dispatch_erased function which handles
    /// the type erasure internally.
    pub command_bus: Option<crate::command::DynCommandBus>,
}

impl SagaServices {
    /// Creates a new SagaServices instance with all services.
    #[inline]
    pub fn new(
        provider_registry: Arc<dyn crate::workers::WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn crate::event_bus::EventBus + Send + Sync>,
        job_repository: Option<Arc<dyn crate::jobs::JobRepository + Send + Sync>>,
        provisioning_service: Option<Arc<dyn crate::workers::WorkerProvisioning + Send + Sync>>,
    ) -> Self {
        Self {
            provider_registry,
            event_bus,
            job_repository,
            provisioning_service,
            orchestrator: None,
            command_bus: None,
        }
    }

    /// Creates a new SagaServices instance with orchestrator support.
    /// EPIC-46 GAP-20: Full constructor with all services.
    #[inline]
    pub fn with_orchestrator(
        provider_registry: Arc<dyn crate::workers::WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn crate::event_bus::EventBus + Send + Sync>,
        job_repository: Option<Arc<dyn crate::jobs::JobRepository + Send + Sync>>,
        provisioning_service: Option<Arc<dyn crate::workers::WorkerProvisioning + Send + Sync>>,
        orchestrator: Option<Arc<dyn SagaOrchestrator<Error = crate::saga::OrchestratorError> + Send + Sync>>,
    ) -> Self {
        Self {
            provider_registry,
            event_bus,
            job_repository,
            provisioning_service,
            orchestrator,
            command_bus: None,
        }
    }

    /// Creates a new SagaServices instance with command bus support (EPIC-50).
    ///
    /// This constructor includes the type-erased CommandBus for dispatching
    /// commands from saga steps.
    #[inline]
    pub fn with_command_bus(
        provider_registry: Arc<dyn crate::workers::WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn crate::event_bus::EventBus + Send + Sync>,
        job_repository: Option<Arc<dyn crate::jobs::JobRepository + Send + Sync>>,
        provisioning_service: Option<Arc<dyn crate::workers::WorkerProvisioning + Send + Sync>>,
        orchestrator: Option<Arc<dyn SagaOrchestrator<Error = crate::saga::OrchestratorError> + Send + Sync>>,
        command_bus: crate::command::DynCommandBus,
    ) -> Self {
        Self {
            provider_registry,
            event_bus,
            job_repository,
            provisioning_service,
            orchestrator,
            command_bus: Some(command_bus),
        }
    }

    /// Creates a new SagaServices instance without provisioning service.
    ///
    /// This is a convenience constructor for sagas that don't need
    /// worker provisioning (e.g., execution, recovery sagas).
    #[inline]
    pub fn without_provisioning(
        provider_registry: Arc<dyn crate::workers::WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn crate::event_bus::EventBus + Send + Sync>,
        job_repository: Option<Arc<dyn crate::jobs::JobRepository + Send + Sync>>,
    ) -> Self {
        Self::new(provider_registry, event_bus, job_repository, None)
    }

    /// Returns true if a provisioning service is available.
    #[inline]
    pub fn has_provisioning_service(&self) -> bool {
        self.provisioning_service.is_some()
    }

    /// Returns true if an orchestrator is available (EPIC-46 GAP-20).
    #[inline]
    pub fn has_orchestrator(&self) -> bool {
        self.orchestrator.is_some()
    }

    /// Returns true if a command bus is available (EPIC-50).
    #[inline]
    pub fn has_command_bus(&self) -> bool {
        self.command_bus.is_some()
    }

    /// Gets a reference to the command bus, if available (EPIC-50).
    #[inline]
    pub fn command_bus(&self) -> Option<&crate::command::DynCommandBus> {
        self.command_bus.as_ref()
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

    /// Executes a saga directly from its context (EPIC-42: Reactive Saga Processing)
    ///
    /// This method looks up the saga type from the context and executes it.
    ///
    /// # Arguments
    /// * `context` - The saga context with metadata
    ///
    /// # Returns
    /// * `Result<SagaExecutionResult, Self::Error>` - The result of execution
    async fn execute(&self, context: &SagaContext) -> Result<SagaExecutionResult, Self::Error>;

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

        // New signature: context instead of output (EPIC-SAGA-ENGINE)
        async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
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
        assert_eq!(SagaType::Provisioning.as_str(), "PROVISIONING");
    }

    #[test]
    fn saga_type_should_have_execution_variant() {
        assert_eq!(SagaType::Execution.as_str(), "EXECUTION");
    }

    #[test]
    fn saga_type_should_have_recovery_variant() {
        assert_eq!(SagaType::Recovery.as_str(), "RECOVERY");
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

    // ============ Idempotency Tests ============

    #[test]
    fn saga_id_for_job_should_be_deterministic() {
        let job_id = "job-12345";
        let saga_id_1 = saga_id_for_job(job_id);
        let saga_id_2 = saga_id_for_job(job_id);
        assert_eq!(saga_id_1, saga_id_2);
    }

    #[test]
    fn saga_id_for_job_should_be_unique_per_job() {
        let saga_id_1 = saga_id_for_job("job-111");
        let saga_id_2 = saga_id_for_job("job-222");
        assert_ne!(saga_id_1, saga_id_2);
    }

    #[test]
    fn saga_id_for_job_should_not_be_nil() {
        let saga_id = saga_id_for_job("job-test");
        assert!(!saga_id.is_nil());
    }

    #[test]
    fn saga_id_for_job_should_use_uuid_v5() {
        let saga_id = saga_id_for_job("job-123");
        // Verify it's a valid v5 UUID by checking the version field
        // v5 UUIDs have the version bits set to 0101 (5)
        let bytes = saga_id.as_bytes();
        let version = (bytes[6] & 0xF0) >> 4;
        assert_eq!(version, 5, "Expected v5 UUID version bits");
    }

    #[test]
    fn saga_id_for_provisioning_should_be_deterministic() {
        let provider_id = "docker-local";
        let job_id = "job-12345";
        let saga_id_1 = saga_id_for_provisioning(provider_id, job_id);
        let saga_id_2 = saga_id_for_provisioning(provider_id, job_id);
        assert_eq!(saga_id_1, saga_id_2);
    }

    #[test]
    fn saga_id_for_provisioning_should_differ_by_provider() {
        let job_id = "job-123";
        let saga_id_docker = saga_id_for_provisioning("docker", job_id);
        let saga_id_k8s = saga_id_for_provisioning("kubernetes", job_id);
        assert_ne!(saga_id_docker, saga_id_k8s);
    }

    #[test]
    fn saga_id_for_recovery_should_be_deterministic() {
        let job_id = "job-12345";
        let saga_id_1 = saga_id_for_recovery(job_id);
        let saga_id_2 = saga_id_for_recovery(job_id);
        assert_eq!(saga_id_1, saga_id_2);
    }

    #[test]
    fn saga_id_for_recovery_should_differ_from_execution() {
        let job_id = "job-123";
        let execution_saga_id = saga_id_for_job(job_id);
        let recovery_saga_id = saga_id_for_recovery(job_id);
        assert_ne!(execution_saga_id, recovery_saga_id);
    }
}
