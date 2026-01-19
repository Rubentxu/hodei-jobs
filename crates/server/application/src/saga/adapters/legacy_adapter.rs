//! Saga Adapters for migration to saga-engine v4.0
//!
//! This module provides adapters that wrap the legacy saga implementation
//! and the saga-engine v4.0 library.

use async_trait::async_trait;
use hodei_server_domain::saga::{
    SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator as DomainSagaOrchestrator, SagaType,
};
use std::io;
use std::sync::Arc;
use tracing::debug;

use hodei_server_domain::DomainError;

use crate::saga::port::SagaPort;
use crate::saga::port::types::{SagaExecutionId, WorkflowState};

/// Adapter that wraps the legacy saga orchestrator to implement SagaPort.
pub struct LegacySagaAdapter {
    orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>,
}

impl LegacySagaAdapter {
    /// Create a new LegacySagaAdapter
    pub fn new(
        orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        Self { orchestrator }
    }
}

#[async_trait]
impl SagaPort for LegacySagaAdapter {
    type Error = io::Error;

    async fn start_workflow(
        &self,
        saga_type: SagaType,
        _input: (),
        _idempotency_key: Option<String>,
    ) -> Result<SagaExecutionId, Self::Error> {
        debug!(saga_type = ?saga_type, "Starting workflow via LegacySagaAdapter");
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "LegacySagaAdapter.start_workflow requires saga-specific implementation",
        ))
    }

    async fn get_workflow_state(
        &self,
        execution_id: &SagaExecutionId,
    ) -> Result<WorkflowState<()>, Self::Error> {
        debug!(execution_id = %execution_id, "Getting workflow state");
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "LegacySagaAdapter.get_workflow_state requires saga-specific implementation",
        ))
    }

    async fn cancel_workflow(
        &self,
        execution_id: &SagaExecutionId,
        reason: String,
    ) -> Result<(), Self::Error> {
        debug!(execution_id = %execution_id, reason = %reason, "Cancelling workflow");
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "LegacySagaAdapter.cancel_workflow requires saga-specific implementation",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::{Saga, SagaOrchestrator};

    struct TestSaga;

    impl Saga for TestSaga {
        fn saga_type(&self) -> SagaType {
            SagaType::Execution
        }

        fn steps(&self) -> Vec<Box<dyn hodei_server_domain::saga::SagaStep<Output = ()>>> {
            vec![]
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
        let _adapter = LegacySagaAdapter::new(orchestrator);
    }
}
