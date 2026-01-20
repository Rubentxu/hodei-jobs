//! Saga Port Abstraction Layer
//!
//! This module defines the SagaPort trait that abstracts saga execution,
//! enabling decoupling between the application layer and saga implementation.

use async_trait::async_trait;
use hodei_server_domain::saga::{SagaContext, SagaExecutionResult, SagaId, SagaType};
use saga_engine_core::workflow::WorkflowDefinition;
use std::fmt::Debug;
use std::time::Duration;

pub mod types;

/// Trait that defines the port interface for saga execution.
///
/// This port provides a simplified interface for saga execution that can be
/// implemented by both the legacy saga orchestrator and saga-engine v4.0.
///
/// # Generic Parameter
///
/// - `W`: The workflow definition type that this port handles
#[async_trait]
pub trait SagaPort<W: WorkflowDefinition>: Send + Sync {
    /// Error type for saga port operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start a new workflow execution.
    async fn start_workflow(
        &self,
        input: serde_json::Value,
        idempotency_key: Option<String>,
    ) -> Result<types::SagaExecutionId, Self::Error>;

    /// Get the current state of a workflow execution.
    async fn get_workflow_state(
        &self,
        execution_id: &types::SagaExecutionId,
    ) -> Result<types::WorkflowState, Self::Error>;

    /// Cancel a running workflow.
    async fn cancel_workflow(
        &self,
        execution_id: &types::SagaExecutionId,
        reason: String,
    ) -> Result<(), Self::Error>;

    /// Send a signal to a workflow (for workflows that support signals).
    async fn send_signal(
        &self,
        execution_id: &types::SagaExecutionId,
        signal: String,
        payload: serde_json::Value,
    ) -> Result<(), Self::Error>;
}

/// Extension trait for SagaPort with convenience methods
#[async_trait]
pub trait SagaPortExt<W: WorkflowDefinition>: SagaPort<W> {
    /// Check if a workflow is currently running
    async fn is_running(&self, execution_id: &types::SagaExecutionId) -> Result<bool, Self::Error> {
        let state = self.get_workflow_state(execution_id).await?;
        Ok(matches!(state, types::WorkflowState::Running { .. }))
    }
}
