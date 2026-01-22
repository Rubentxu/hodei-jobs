//! Saga Port Abstraction Layer
//!
//! This module defines the SagaPort trait that abstracts saga execution,
//! enabling decoupling between the application layer and saga implementation.
//!
//! # Type Safety
//!
//! The port is generic over `W: DurableWorkflow`, which provides compile-time
//! type safety for inputs and outputs. This eliminates runtime serialization
//! errors and provides better developer experience.
//!
//! # Example
//!
//! ```rust,ignore
//! use saga_engine_core::workflow::DurableWorkflow;
//! use hodei_server_application::saga::port::SagaPort;
//! use hodei_server_application::saga::workflows::recovery_durable::RecoveryWorkflow;
//!
//! // Type-safe access to workflow input/output
//! async fn start_recovery(port: &impl SagaPort<RecoveryWorkflow>) {
//!     let input = RecoveryWorkflow::Input::new(/* ... */);
//!     let id = port.start_workflow(input, None).await?;
//! }
//! ```

use async_trait::async_trait;
use saga_engine_core::workflow::{DurableWorkflow, WorkflowState};
use std::fmt::Debug;
use std::time::Duration;

pub mod types;

/// Trait that defines the port interface for saga execution.
///
/// This port provides a type-safe interface for saga execution that can be
/// implemented by saga-engine v4.0. The generic parameter `W` ensures that
/// inputs and outputs are type-checked at compile time.
///
/// # Generic Parameter
///
/// - `W`: The durable workflow type that this port handles. This provides
///   type-safe access to `W::Input` and `W::Output`.
///
#[async_trait]
pub trait SagaPort<W: DurableWorkflow>: Send + Sync {
    /// Error type for saga port operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start a new workflow execution.
    ///
    /// This method takes a type-safe input from `W::Input` and returns
    /// an execution ID that can be used to track progress.
    ///
    /// # Arguments
    ///
    /// * `input` - The workflow input (type-safe via `W::Input`)
    /// * `idempotency_key` - Optional key to ensure idempotency
    ///
    /// # Returns
    ///
    /// The unique execution ID for this workflow run.
    async fn start_workflow(
        &self,
        input: W::Input,
        idempotency_key: Option<String>,
    ) -> Result<types::SagaExecutionId, Self::Error>;

    /// Get the current state of a workflow execution.
    async fn get_workflow_state(
        &self,
        execution_id: &types::SagaExecutionId,
    ) -> Result<WorkflowState, Self::Error>;

    /// Cancel a running workflow.
    ///
    /// This initiates the compensation/rollback process for any completed steps.
    async fn cancel_workflow(
        &self,
        execution_id: &types::SagaExecutionId,
        reason: String,
    ) -> Result<(), Self::Error>;

    /// Send a signal to a workflow (for workflows that support signals).
    ///
    /// Signals allow external systems to interact with long-running workflows.
    async fn send_signal(
        &self,
        execution_id: &types::SagaExecutionId,
        signal: String,
        payload: W::Output, // Using Output type for signal payload
    ) -> Result<(), Self::Error>;

    /// Wait for a workflow to complete and return its final state.
    ///
    /// This is useful for synchronous workflows that need to wait for results.
    async fn wait_for_completion(
        &self,
        execution_id: &types::SagaExecutionId,
        timeout: Duration,
    ) -> Result<WorkflowState, Self::Error>;
}

/// Extension trait for SagaPort with convenience methods
#[async_trait]
pub trait SagaPortExt<W: DurableWorkflow>: SagaPort<W> {
    /// Check if a workflow is currently running
    async fn is_running(&self, execution_id: &types::SagaExecutionId) -> Result<bool, Self::Error> {
        let state = self.get_workflow_state(execution_id).await?;
        Ok(matches!(state, WorkflowState::Running { .. }))
    }

    /// Wait for a workflow to complete with default timeout
    async fn wait_for_completion_default(
        &self,
        execution_id: &types::SagaExecutionId,
    ) -> Result<WorkflowState, Self::Error> {
        self.wait_for_completion(execution_id, Duration::from_secs(300))
            .await
    }
}
