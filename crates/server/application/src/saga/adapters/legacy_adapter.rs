//! Saga Adapters for migration to saga-engine v4.0
//!
//! This module provides adapters that wrap the legacy saga implementation
//! and the saga-engine v4.0 library.

use async_trait::async_trait;
use hodei_server_domain::saga::{
    SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator as DomainSagaOrchestrator, SagaType,
};
use saga_engine_core::workflow::WorkflowDefinition;
use std::io;
use std::sync::Arc;
use tracing::debug;

use hodei_server_domain::DomainError;

use crate::saga::port::SagaPort;
use crate::saga::port::types::{SagaExecutionId, WorkflowState};

/// Adapter that wraps the legacy saga orchestrator to implement SagaPort for a specific workflow.
///
/// This adapter translates between the saga-engine v4.0 WorkflowDefinition and the legacy saga system.
pub struct LegacySagaAdapter<W: WorkflowDefinition> {
    orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>,
    _phantom: std::marker::PhantomData<W>,
}

impl<W: WorkflowDefinition> LegacySagaAdapter<W> {
    /// Create a new LegacySagaAdapter
    pub fn new(
        orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        Self {
            orchestrator,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<W: WorkflowDefinition> SagaPort<W> for LegacySagaAdapter<W> {
    type Error = io::Error;

    async fn start_workflow(
        &self,
        input: serde_json::Value,
        _idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error> {
        debug!(
            "Starting workflow via LegacySagaAdapter for workflow type {}",
            W::TYPE_ID
        );

        // For now, return Unsupported since this requires saga-specific implementation
        // In production, this would translate W::Input to legacy command
        // and execute the saga via orchestrator
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "LegacySagaAdapter.start_workflow for {} requires saga-specific implementation",
                W::TYPE_ID
            ),
        ))
    }

    async fn get_workflow_state(
        &self,
        execution_id: &SagaExecutionId,
    ) -> Result<WorkflowState, Self::Error> {
        debug!(execution_id = %execution_id, "Getting workflow state from LegacySagaAdapter");

        // For now, return Unsupported since this requires saga-specific implementation
        // In production, this would query the saga via orchestrator and translate to WorkflowState
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "LegacySagaAdapter.get_workflow_state for {} requires saga-specific implementation",
                W::TYPE_ID
            ),
        ))
    }

    async fn cancel_workflow(
        &self,
        execution_id: &SagaExecutionId,
        reason: String,
    ) -> Result<(), Self::Error> {
        debug!(execution_id = %execution_id, reason = %reason, "Cancelling workflow via LegacySagaAdapter");

        // For now, return Unsupported since this requires saga-specific implementation
        // In production, this would cancel the saga via orchestrator
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "LegacySagaAdapter.cancel_workflow for {} requires saga-specific implementation",
                W::TYPE_ID
            ),
        ))
    }

    async fn send_signal(
        &self,
        _execution_id: &SagaExecutionId,
        _signal: String,
        _payload: serde_json::Value,
    ) -> Result<(), Self::Error> {
        debug!("Sending signal via LegacySagaAdapter");

        // Legacy system doesn't support signals in the same way
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "LegacySagaAdapter.send_signal for {} not supported in legacy system",
                W::TYPE_ID
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::{Saga, SagaOrchestrator};

    // Mock workflow definition for testing
    struct TestWorkflow;

    impl WorkflowDefinition for TestWorkflow {
        const TYPE_ID: &'static str = "test-workflow";
        const VERSION: u32 = 1;

        fn steps(&self) -> &[Box<dyn saga_engine_core::workflow::DynWorkflowStep>] {
            &[]
        }

        fn configuration(&self) -> saga_engine_core::workflow::WorkflowConfig {
            saga_engine_core::workflow::WorkflowConfig::default()
        }
    }

    #[derive(Clone)]
    struct MockOrchestrator;

    #[async_trait::async_trait]
    impl DomainSagaOrchestrator for MockOrchestrator {
        type Error = DomainError;

        async fn execute_saga(
            &self,
            _saga: &dyn Saga,
            _context: SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            unreachable!()
        }

        async fn get_saga(
            &self,
            _saga_id: &SagaId,
        ) -> std::result::Result<Option<SagaContext>, Self::Error> {
            Ok(None)
        }

        async fn cancel_saga(&self, _saga_id: &SagaId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn execute(
            &self,
            _context: &SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn test_legacy_saga_adapter_creation() {
        let orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync> =
            Arc::new(MockOrchestrator);
        let _adapter = LegacySagaAdapter::<TestWorkflow>::new(orchestrator);
    }
}
