//! Saga Context V2 - Refactored SagaContext
//!
//! This module contains the refactored SagaContext implementation
//! following SOLID principles and DDD best practices.
//!
//! # Architecture
//!
//! The old SagaContext mixed 7 responsibilities. This module separates them into:
//! - **SagaIdentity**: Value object for saga identification (immutable)
//! - **SagaExecutionState**: Value object for execution state (mutable)
//! - **SagaMetadata**: Trait for type-safe metadata
//! - **StepOutputs**: Type-safe step output storage
//! - **SagaContextV2**: Simplified context using the above components
//!
//! # Migration Strategy
//!
//! This is a gradual migration using the Strangler Fig pattern:
//! 1. New code uses SagaContextV2
//! 2. Old code continues using SagaContext
//! 3. Eventually, SagaContext will be deprecated

use crate::saga::types::SagaContext as SagaContextV1;
use crate::saga::{SagaId, SagaState, SagaType};
use crate::shared_kernel::{JobId, WorkerId};
use crate::workers::WorkerSpec;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

// =============================================================================
// VALUE OBJECTS
// ============================================================================

/// Saga Phase - Forward execution or Compensation
///
/// Replaces the boolean `is_compensating` with a clearer enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaPhase {
    /// Executing forward steps
    Forward,
    /// Compensating previously executed steps
    Compensating,
}

/// Saga Identity - Value Object (Immutable)
///
/// Contains all identification-related fields for a Saga.
/// This is a Value Object - it has no identity separate from its values.
///
/// # Design Principles
///
/// - **Immutable**: All fields are public but the struct provides builder methods
/// - **Value Object**: Equality based on all fields, not on identity
/// - **Type Safety**: Uses specific types (CorrelationId, Actor) instead of String
///
/// # Example
///
/// ```ignore
/// let identity = SagaIdentity::new(saga_id, saga_type)
///     .with_correlation_id(CorrelationId::new(id))
///     .with_actor(Actor::new("user-123"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaIdentity {
    /// Unique identifier for this saga instance
    pub id: SagaId,
    /// Type of saga being executed
    pub type_: SagaType,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<CorrelationId>,
    /// Actor that initiated the saga
    pub actor: Option<Actor>,
    /// When the saga started
    pub started_at: DateTime<Utc>,
    /// W3C Trace Context for distributed tracing propagation
    pub trace_parent: Option<TraceParent>,
}

impl SagaIdentity {
    /// Create a new SagaIdentity with required fields
    pub fn new(id: SagaId, type_: SagaType) -> Self {
        Self {
            id,
            type_,
            correlation_id: None,
            actor: None,
            started_at: Utc::now(),
            trace_parent: None,
        }
    }

    /// Add correlation ID
    pub fn with_correlation_id(mut self, id: CorrelationId) -> Self {
        self.correlation_id = Some(id);
        self
    }

    /// Add actor
    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Add trace parent
    pub fn with_trace_parent(mut self, trace: TraceParent) -> Self {
        self.trace_parent = Some(trace);
        self
    }
}

/// Saga Execution State - Value Object (Mutable)
///
/// Contains all execution-related state for a Saga.
/// This is a Value Object that tracks the execution progress.
///
/// # Design Principles
///
/// - **Mutable**: State changes during execution
/// - **Self-validating**: Methods enforce valid state transitions
/// - **Optimistic Locking**: Version field for concurrency control
///
/// # Example
///
/// ```ignore
/// let mut state = SagaExecutionState::new();
/// state.advance()?;  // Move to next step
/// state.start_compensation();  // Start compensating
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaExecutionState {
    /// Current step being executed
    pub current_step: usize,
    /// Current phase (forward or compensation)
    pub phase: SagaPhase,
    /// Current saga state
    pub state: SagaState,
    /// Version for optimistic locking
    pub version: u64,
    /// Error message if saga failed
    pub error_message: Option<String>,
}

impl SagaExecutionState {
    /// Create a new execution state in initial state
    pub fn new() -> Self {
        Self {
            current_step: 0,
            phase: SagaPhase::Forward,
            state: SagaState::Pending,
            version: 0,
            error_message: None,
        }
    }

    /// Advance to the next step
    ///
    /// Increments both step and version for optimistic locking.
    pub fn advance(&mut self) -> Result<(), String> {
        self.current_step += 1;
        self.version += 1;
        Ok(())
    }

    /// Start compensation phase
    pub fn start_compensation(&mut self) {
        self.phase = SagaPhase::Compensating;
        self.version += 1;
    }

    /// Mark the saga as failed
    pub fn fail(&mut self, error: String) {
        self.state = SagaState::Failed;
        self.error_message = Some(error);
        self.version += 1;
    }

    /// Mark the saga as completed
    pub fn complete(&mut self) {
        self.state = SagaState::Completed;
        self.version += 1;
    }
}

impl Default for SagaExecutionState {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TYPED METADATA
// ============================================================================

/// Trait for type-safe saga metadata
///
/// Each saga type can implement this trait to provide
/// type-safe metadata instead of HashMap<String, serde_json::Value>.
///
/// # Type Safety
///
/// This trait uses `Any` for downcasting, enabling:
/// ```ignore
/// let metadata = context.get_metadata::<ProvisioningMetadata>();
/// ```
pub trait SagaMetadata: Send + Sync + 'static {
    /// Get metadata as Any for downcasting
    fn as_any(&self) -> &dyn Any;
}

/// Default empty metadata implementation
#[derive(Debug, Clone)]
pub struct DefaultSagaMetadata;

impl SagaMetadata for DefaultSagaMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Provisioning saga metadata
///
/// Contains type-safe metadata for provisioning operations.
#[derive(Debug, Clone)]
pub struct ProvisioningMetadata {
    /// Provider selected for this provisioning
    pub provider_id: String,
    /// Number of retries attempted
    pub retry_count: u32,
    /// Last error if retry failed
    pub last_error: Option<String>,
    /// Worker specification
    pub worker_spec: WorkerSpec,
}

impl SagaMetadata for ProvisioningMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Execution saga metadata
///
/// Contains type-safe metadata for job execution operations.
#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    /// Job being executed
    pub job_id: JobId,
    /// Worker assigned to the job
    pub worker_id: Option<WorkerId>,
}

impl SagaMetadata for ExecutionMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Recovery saga metadata
///
/// Contains type-safe metadata for worker recovery operations.
#[derive(Debug, Clone)]
pub struct RecoveryMetadata {
    /// Worker being recovered
    pub worker_id: WorkerId,
    /// Reason for recovery
    pub reason: String,
}

impl SagaMetadata for RecoveryMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// =============================================================================
// TYPED STEP OUTPUTS
// ============================================================================

/// Type-safe step output storage
///
/// Replaces HashMap<String, serde_json::Value> with typed values.
/// This provides compile-time type safety and better performance.
///
/// # Example
///
/// ```ignore
/// let mut outputs = StepOutputs::new();
/// outputs.set("worker_id".to_string(), StepOutputValue::WorkerId(worker_id));
/// let worker_id = outputs.get_worker_id("worker_id")?;
/// ```
pub struct StepOutputs {
    inner: HashMap<String, StepOutputValue>,
}

/// Typed step output value
///
/// Instead of serde_json::Value, we use an enum with known variants.
/// This provides type safety and better performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepOutputValue {
    /// Worker ID output
    WorkerId(WorkerId),
    /// Provider ID output
    ProviderId(String),
    /// String output
    String(String),
    /// U32 output
    U32(u32),
    /// U64 output
    U64(u64),
    /// Boolean output
    Bool(bool),
}

impl StepOutputs {
    /// Create a new empty StepOutputs
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Set a step output value
    pub fn set(&mut self, key: String, value: StepOutputValue) {
        self.inner.insert(key, value);
    }

    /// Get a worker ID from outputs
    pub fn get_worker_id(&self, key: &str) -> Option<&WorkerId> {
        self.inner.get(key).and_then(|v| match v {
            StepOutputValue::WorkerId(id) => Some(id),
            _ => None,
        })
    }

    /// Get a string value from outputs
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.inner.get(key).and_then(|v| match v {
            StepOutputValue::String(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get a U32 value from outputs
    pub fn get_u32(&self, key: &str) -> Option<u32> {
        self.inner.get(key).and_then(|v| match v {
            StepOutputValue::U32(n) => Some(*n),
            _ => None,
        })
    }

    /// Get a bool value from outputs
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.inner.get(key).and_then(|v| match v {
            StepOutputValue::Bool(b) => Some(*b),
            _ => None,
        })
    }

    /// Check if outputs contains a key
    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Get the number of stored outputs
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if outputs is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for StepOutputs {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// VALUE OBJECTS - Supporting Types
// ============================================================================

/// Correlation ID for distributed tracing
///
/// Wrapper around String to provide type safety and prevent confusion.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    /// Create a new correlation ID
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Get the inner value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Actor who initiated the saga
///
/// Wrapper around String to provide type safety.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Actor(pub String);

impl Actor {
    /// Create a new actor
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Get the inner value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Trace Parent for W3C distributed tracing
///
/// Wrapper around String to provide type safety.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceParent(pub String);

impl TraceParent {
    /// Create a new trace parent
    pub fn new(trace: String) -> Self {
        Self(trace)
    }

    /// Get the inner value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// =============================================================================
// SAGACONTEXT V2 - Refactored Context
// ============================================================================

/// SagaContext V2 - Refactored with separated concerns
///
/// This is the new, improved version of SagaContext that follows
/// SOLID principles and DDD best practices.
///
/// # Design Principles
///
/// - **SRP**: Each component has a single responsibility
/// - **ISP**: Clients depend only on what they need
/// - **DIP**: No Service Locator anti-pattern
/// - **Type Safety**: Uses Value Objects and typed metadata
///
/// # Generic Parameter
///
/// `M` is the metadata type, defaulting to `DefaultSagaMetadata`.
/// Each saga type can provide its own metadata type.
///
/// # Example
///
/// ```ignore
/// // Create with ProvisioningMetadata
/// let context = SagaContextV2::new(
///     saga_id,
///     SagaType::Provisioning,
///     ProvisioningMetadata { ... },
/// );
///
/// // Access identity
/// let id = context.id();
///
/// // Access execution state
/// context.advance()?;
///
/// // Access typed metadata
/// let provider_id = &context.metadata.provider_id;
/// ```
pub struct SagaContextV2<M: SagaMetadata = DefaultSagaMetadata> {
    /// Saga identity (immutable)
    pub identity: SagaIdentity,

    /// Execution state (mutable)
    pub execution: SagaExecutionState,

    /// Step outputs for compensation
    pub outputs: StepOutputs,

    /// Type-safe metadata specific to saga type
    pub metadata: M,
}

impl<M: SagaMetadata> SagaContextV2<M> {
    /// Create a new SagaContextV2
    pub fn new(id: SagaId, type_: SagaType, metadata: M) -> Self {
        Self {
            identity: SagaIdentity::new(id, type_),
            execution: SagaExecutionState::new(),
            outputs: StepOutputs::new(),
            metadata,
        }
    }

    // ========================================================================
    // IDENTITY METHODS (delegated to identity)
    // ========================================================================

    /// Get the saga ID
    pub fn id(&self) -> &SagaId {
        &self.identity.id
    }

    /// Get the saga type
    pub fn type_(&self) -> &SagaType {
        &self.identity.type_
    }

    /// Get correlation ID if present
    pub fn correlation_id(&self) -> Option<&CorrelationId> {
        self.identity.correlation_id.as_ref()
    }

    /// Get actor if present
    pub fn actor(&self) -> Option<&Actor> {
        self.identity.actor.as_ref()
    }

    /// Get started_at timestamp
    pub fn started_at(&self) -> &DateTime<Utc> {
        &self.identity.started_at
    }

    /// Get trace parent if present
    pub fn trace_parent(&self) -> Option<&TraceParent> {
        self.identity.trace_parent.as_ref()
    }

    // ========================================================================
    // EXECUTION STATE METHODS (delegated to execution)
    // ========================================================================

    /// Get current step number
    pub fn current_step(&self) -> usize {
        self.execution.current_step
    }

    /// Get current phase
    pub fn phase(&self) -> SagaPhase {
        self.execution.phase
    }

    /// Get current state
    pub fn state(&self) -> &SagaState {
        &self.execution.state
    }

    /// Get version number
    pub fn version(&self) -> u64 {
        self.execution.version
    }

    /// Get error message if present
    pub fn error_message(&self) -> Option<&str> {
        self.execution.error_message.as_deref()
    }

    /// Advance to the next step
    pub fn advance(&mut self) -> Result<(), String> {
        self.execution.advance()
    }

    /// Start compensation phase
    pub fn start_compensation(&mut self) {
        self.execution.start_compensation()
    }

    /// Mark as failed
    pub fn fail(&mut self, error: String) {
        self.execution.fail(error)
    }

    /// Mark as completed
    pub fn complete(&mut self) {
        self.execution.complete()
    }

    // ========================================================================
    // OUTPUTS METHODS
    // ========================================================================

    /// Set a step output value
    pub fn set_output(&mut self, key: String, value: StepOutputValue) {
        self.outputs.set(key, value);
    }

    /// Get worker ID from outputs
    pub fn get_worker_id(&self, key: &str) -> Option<&WorkerId> {
        self.outputs.get_worker_id(key)
    }

    /// Get string value from outputs
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.outputs.get_string(key)
    }

    /// Check if outputs contains a key
    pub fn has_output(&self, key: &str) -> bool {
        self.outputs.contains(key)
    }

    // ========================================================================
    // CONVERSION METHODS
    // ========================================================================

    /// Get typed metadata
    ///
    /// # Type Parameters
    ///
    /// * `T` - The metadata type to cast to
    ///
    /// # Returns
    ///
    /// * `Some(&T)` if the metadata is of type T
    /// * `None` if the metadata is not of type T
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(meta) = context.get_metadata::<ProvisioningMetadata>() {
    ///     println!("Provider: {}", meta.provider_id);
    /// }
    /// ```
    pub fn get_metadata<T: 'static>(&self) -> Option<&T> {
        self.metadata.as_any().downcast_ref::<T>()
    }
}

// =============================================================================
// BUILDER - For convenient construction
// ============================================================================

/// Builder for SagaContextV2
///
/// Provides a fluent API for constructing SagaContextV2 instances.
///
/// # Example
///
/// ```ignore
/// let context = SagaContextV2Builder::new()
///     .with_id(saga_id)
///     .with_type(SagaType::Provisioning)
///     .with_correlation_id(CorrelationId::new(correlation_id))
///     .with_metadata(provisioning_metadata)
///     .build()?;
/// ```
pub struct SagaContextV2Builder<M: SagaMetadata = DefaultSagaMetadata> {
    identity: Option<SagaIdentity>,
    metadata: Option<M>,
}

impl<M: SagaMetadata> SagaContextV2Builder<M> {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            identity: None,
            metadata: None,
        }
    }

    /// Set saga ID
    pub fn with_id(mut self, id: SagaId) -> Self {
        self.identity = Some(SagaIdentity::new(id, SagaType::Provisioning));
        self
    }

    /// Set saga type
    pub fn with_type(mut self, type_: SagaType) -> Self {
        if let Some(ref mut identity) = self.identity {
            identity.type_ = type_;
        }
        self
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, id: CorrelationId) -> Self {
        if let Some(ref mut identity) = self.identity {
            identity.correlation_id = Some(id);
        }
        self
    }

    /// Set actor
    pub fn with_actor(mut self, actor: Actor) -> Self {
        if let Some(ref mut identity) = self.identity {
            identity.actor = Some(actor);
        }
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: M) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Build the SagaContextV2
    pub fn build(self) -> Result<SagaContextV2<M>, String> {
        let identity = self.identity.ok_or("Missing identity".to_string())?;
        let metadata = self.metadata.ok_or("Missing metadata".to_string())?;

        Ok(SagaContextV2 {
            identity,
            execution: SagaExecutionState::new(),
            outputs: StepOutputs::new(),
            metadata,
        })
    }
}

impl<M: SagaMetadata> Default for SagaContextV2Builder<M> {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// CONVERSION - V1 <-> V2 Interoperability
// ============================================================================

/// Conversion methods between SagaContext V1 and V2
///
/// These methods enable gradual migration by allowing bidirectional
/// conversion between the legacy and refactored implementations.
impl SagaContextV2<DefaultSagaMetadata> {
    /// Convert from SagaContext V1 to V2
    ///
    /// This method creates a V2 context from a V1 context,
    /// preserving all state and metadata.
    ///
    /// # Arguments
    ///
    /// * `v1` - The V1 context to convert from
    ///
    /// # Returns
    ///
    /// A new V2 context with the same state, using Default metadata
    pub fn from_v1(v1: &SagaContextV1) -> Self {
        // Build identity from V1 fields
        let mut identity_builder = SagaIdentity::new(v1.saga_id.clone(), v1.saga_type);

        if let Some(ref corr_id) = v1.correlation_id {
            identity_builder =
                identity_builder.with_correlation_id(CorrelationId::new(corr_id.clone()));
        }

        if let Some(ref actor) = v1.actor {
            identity_builder = identity_builder.with_actor(Actor::new(actor.clone()));
        }

        if let Some(ref trace_parent) = v1.trace_parent {
            identity_builder =
                identity_builder.with_trace_parent(TraceParent::new(trace_parent.clone()));
        }

        // Build execution state from V1 fields
        let phase = if v1.is_compensating {
            SagaPhase::Compensating
        } else {
            SagaPhase::Forward
        };

        let execution = SagaExecutionState {
            current_step: v1.current_step,
            phase,
            state: v1.state,
            version: v1.version,
            error_message: v1.error_message.clone(),
        };

        // Note: step_outputs cannot be converted from V1 since the field is private
        // In production, add a public getter method to SagaContext V1
        // For now, we start with empty outputs - new sagas will populate them
        let outputs = StepOutputs::new();

        Self {
            identity: identity_builder,
            execution,
            outputs,
            metadata: DefaultSagaMetadata,
        }
    }

    /// Convert from V2 to V1 (legacy format)
    ///
    /// This method creates a V1 context from a V2 context,
    /// enabling backward compatibility during migration.
    ///
    /// # Returns
    ///
    /// A new V1 context with the same state
    pub fn to_v1(&self) -> SagaContextV1 {
        let correlation_id = self.identity.correlation_id.as_ref().map(|id| id.0.clone());
        let actor = self.identity.actor.as_ref().map(|a| a.0.clone());

        // Use public constructor to create V1 context
        let mut v1 = SagaContextV1::new(
            self.identity.id.clone(),
            self.identity.type_,
            correlation_id,
            actor,
        );

        // Set additional fields directly since SagaContext has public fields
        v1.current_step = self.execution.current_step;
        v1.is_compensating = matches!(self.execution.phase, SagaPhase::Compensating);
        v1.state = self.execution.state;
        v1.version = self.execution.version;
        v1.error_message = self.execution.error_message.clone();

        if let Some(ref trace_parent) = self.identity.trace_parent {
            v1.trace_parent = Some(trace_parent.0.clone());
        }

        v1
    }
}

/// Conversion methods for SagaContextV2 with custom metadata
impl<M: SagaMetadata> SagaContextV2<M> {
    /// Convert from SagaContext V1 to V2 with custom metadata
    ///
    /// This method creates a V2 context from a V1 context,
    /// using the provided custom metadata type.
    ///
    /// # Arguments
    ///
    /// * `v1` - The V1 context to convert from
    /// * `metadata` - The custom metadata to use
    ///
    /// # Returns
    ///
    /// A new V2 context with the provided metadata
    pub fn from_v1_with_metadata(v1: &SagaContextV1, metadata: M) -> Self {
        // Build identity from V1 fields
        let mut identity_builder = SagaIdentity::new(v1.saga_id.clone(), v1.saga_type);

        if let Some(ref corr_id) = v1.correlation_id {
            identity_builder =
                identity_builder.with_correlation_id(CorrelationId::new(corr_id.clone()));
        }

        if let Some(ref actor) = v1.actor {
            identity_builder = identity_builder.with_actor(Actor::new(actor.clone()));
        }

        if let Some(ref trace_parent) = v1.trace_parent {
            identity_builder =
                identity_builder.with_trace_parent(TraceParent::new(trace_parent.clone()));
        }

        // Build execution state from V1 fields
        let phase = if v1.is_compensating {
            SagaPhase::Compensating
        } else {
            SagaPhase::Forward
        };

        let execution = SagaExecutionState {
            current_step: v1.current_step,
            phase,
            state: v1.state,
            version: v1.version,
            error_message: v1.error_message.clone(),
        };

        // Note: step_outputs cannot be converted from V1 since the field is private
        let outputs = StepOutputs::new();

        Self {
            identity: identity_builder,
            execution,
            outputs,
            metadata,
        }
    }
}

// =============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // SagaIdentity Tests
    // ========================================================================

    #[test]
    fn test_saga_identity_new() {
        let id = SagaId::new();
        let type_ = SagaType::Provisioning;
        let identity = SagaIdentity::new(id.clone(), type_);

        assert_eq!(identity.id, id);
        assert_eq!(identity.type_, SagaType::Provisioning);
        assert!(identity.correlation_id.is_none());
        assert!(identity.actor.is_none());
    }

    #[test]
    fn test_saga_identity_builder() {
        let id = SagaId::new();
        let identity = SagaIdentity::new(id, SagaType::Provisioning)
            .with_correlation_id(CorrelationId::new("corr-123".to_string()))
            .with_actor(Actor::new("user-123".to_string()));

        assert!(identity.correlation_id.is_some());
        assert!(identity.actor.is_some());
    }

    #[test]
    fn test_saga_identity_equality() {
        let id = SagaId::new();
        let identity1 = SagaIdentity::new(id.clone(), SagaType::Provisioning);
        let identity2 = SagaIdentity::new(id, SagaType::Provisioning);

        // Verify ID and type are equal (started_at will differ)
        assert_eq!(identity1.id, identity2.id);
        assert_eq!(identity1.type_, identity2.type_);
        assert_eq!(identity1.correlation_id, identity2.correlation_id);
        assert_eq!(identity1.actor, identity2.actor);
        // Note: started_at will differ due to Utc::now() calls
    }

    // ========================================================================
    // SagaExecutionState Tests
    // ========================================================================

    #[test]
    fn test_execution_state_new() {
        let state = SagaExecutionState::new();

        assert_eq!(state.current_step, 0);
        assert_eq!(state.phase, SagaPhase::Forward);
        assert_eq!(state.state, SagaState::Pending);
        assert_eq!(state.version, 0);
        assert!(state.error_message.is_none());
    }

    #[test]
    fn test_execution_state_advance() {
        let mut state = SagaExecutionState::new();
        state.advance().unwrap();

        assert_eq!(state.current_step, 1);
        assert_eq!(state.version, 1);
    }

    #[test]
    fn test_execution_state_start_compensation() {
        let mut state = SagaExecutionState::new();
        state.start_compensation();

        assert_eq!(state.phase, SagaPhase::Compensating);
        assert_eq!(state.version, 1);
    }

    #[test]
    fn test_execution_state_fail() {
        let mut state = SagaExecutionState::new();
        state.fail("Test error".to_string());

        assert_eq!(state.state, SagaState::Failed);
        assert_eq!(state.error_message, Some("Test error".to_string()));
    }

    #[test]
    fn test_execution_state_complete() {
        let mut state = SagaExecutionState::new();
        state.complete();

        assert_eq!(state.state, SagaState::Completed);
    }

    // ========================================================================
    // StepOutputs Tests
    // ========================================================================

    #[test]
    fn test_step_outputs_new() {
        let outputs = StepOutputs::new();
        assert!(outputs.is_empty());
        assert_eq!(outputs.len(), 0);
    }

    #[test]
    fn test_step_outputs_set_get() {
        let mut outputs = StepOutputs::new();
        let worker_id = WorkerId::new();

        outputs.set(
            "worker_id".to_string(),
            StepOutputValue::WorkerId(worker_id.clone()),
        );

        assert!(outputs.contains("worker_id"));
        assert_eq!(outputs.get_worker_id("worker_id"), Some(&worker_id));
    }

    #[test]
    fn test_step_outputs_typed_accessors() {
        let mut outputs = StepOutputs::new();

        outputs.set("count".to_string(), StepOutputValue::U32(42));
        outputs.set("success".to_string(), StepOutputValue::Bool(true));
        outputs.set(
            "message".to_string(),
            StepOutputValue::String("hello".to_string()),
        );

        assert_eq!(outputs.get_u32("count"), Some(42));
        assert_eq!(outputs.get_bool("success"), Some(true));
        assert_eq!(outputs.get_string("message"), Some("hello"));
    }

    // ========================================================================
    // SagaContextV2 Tests
    // ========================================================================

    #[test]
    fn test_saga_context_v2_new() {
        let id = SagaId::new();
        let metadata = DefaultSagaMetadata;
        let context = SagaContextV2::new(id.clone(), SagaType::Provisioning, metadata);

        assert_eq!(context.id(), &id);
        assert_eq!(context.current_step(), 0);
        assert!(context.outputs.is_empty());
    }

    #[test]
    fn test_saga_context_v2_advance() {
        let id = SagaId::new();
        let mut context = SagaContextV2::new(id, SagaType::Provisioning, DefaultSagaMetadata);

        context.advance().unwrap();
        assert_eq!(context.current_step(), 1);
        assert_eq!(context.version(), 1);
    }

    #[test]
    fn test_saga_context_v2_outputs() {
        let id = SagaId::new();
        let mut context = SagaContextV2::new(id, SagaType::Provisioning, DefaultSagaMetadata);

        let worker_id = WorkerId::new();
        context.set_output(
            "worker_id".to_string(),
            StepOutputValue::WorkerId(worker_id.clone()),
        );

        assert_eq!(context.get_worker_id("worker_id"), Some(&worker_id));
    }

    #[test]
    fn test_saga_context_v2_with_provisioning_metadata() {
        let id = SagaId::new();
        let metadata = ProvisioningMetadata {
            provider_id: "provider-1".to_string(),
            retry_count: 0,
            last_error: None,
            worker_spec: WorkerSpec::new(
                "test-image:latest".to_string(),
                "server:50051".to_string(),
            ),
        };

        let context = SagaContextV2::new(id.clone(), SagaType::Provisioning, metadata);

        assert_eq!(context.id(), &id);
        assert_eq!(context.metadata.provider_id, "provider-1");
    }

    // ========================================================================
    // Builder Tests
    // ========================================================================

    #[test]
    fn test_builder_basic() {
        let id = SagaId::new();
        let metadata = DefaultSagaMetadata;

        let context = SagaContextV2Builder::new()
            .with_id(id.clone())
            .with_metadata(metadata)
            .build()
            .unwrap();

        assert_eq!(context.id(), &id);
    }

    #[test]
    fn test_builder_full() {
        let id = SagaId::new();
        let correlation_id = CorrelationId::new("corr-123".to_string());
        let actor = Actor::new("user-123".to_string());
        let metadata = DefaultSagaMetadata;

        let context = SagaContextV2Builder::new()
            .with_id(id.clone())
            .with_correlation_id(correlation_id)
            .with_actor(actor)
            .with_metadata(metadata)
            .build()
            .unwrap();

        assert_eq!(context.id(), &id);
        assert!(context.correlation_id().is_some());
        assert!(context.actor().is_some());
    }

    #[test]
    fn test_builder_missing_identity() {
        let result: Result<SagaContextV2<DefaultSagaMetadata>, String> =
            SagaContextV2Builder::new().build();

        assert!(result.is_err());
    }

    #[test]
    fn test_builder_missing_metadata() {
        let id = SagaId::new();
        let result: Result<SagaContextV2<DefaultSagaMetadata>, String> =
            SagaContextV2Builder::new().with_id(id).build();

        assert!(result.is_err());
    }
}
