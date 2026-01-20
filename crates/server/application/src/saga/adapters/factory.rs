//! Saga Adapter Factory
//!
//! Factory for creating saga adapters.

use std::sync::Arc;
use tracing::debug;

use hodei_server_domain::DomainError;
use hodei_server_domain::saga::{SagaOrchestrator as DomainSagaOrchestrator, SagaType};
use saga_engine_core::workflow::WorkflowDefinition;

use super::legacy_adapter::LegacySagaAdapter;
use super::v4_adapter::{SagaEngineV4Adapter, SagaEngineV4Config, WorkflowRuntime};

use crate::saga::port::SagaPort;
use crate::saga::port::types::{SagaExecutionId, WorkflowState};

/// Types of saga adapters available
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SagaAdapterType {
    /// Legacy saga orchestrator adapter
    Legacy,
    /// SagaEngine v4.0 library adapter
    V4,
}

impl Default for SagaAdapterType {
    fn default() -> Self {
        SagaAdapterType::Legacy
    }
}

impl std::fmt::Display for SagaAdapterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SagaAdapterType::Legacy => write!(f, "legacy"),
            SagaAdapterType::V4 => write!(f, "v4"),
        }
    }
}

/// Factory for creating saga adapters.
#[derive(Clone)]
pub struct SagaAdapterFactory<R: WorkflowRuntime + Send + Sync + 'static> {
    /// Legacy saga orchestrator
    orchestrator: Option<Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>>,
    /// SagaEngine v4.0 runtime
    v4_runtime: Option<Arc<R>>,
    /// Default adapter type
    default_adapter_type: SagaAdapterType,
    /// V4 configuration
    v4_config: Option<SagaEngineV4Config>,
}

impl<R: WorkflowRuntime + Send + Sync + Default + 'static> Default for SagaAdapterFactory<R> {
    fn default() -> Self {
        Self::new(None, None)
    }
}

impl<R: WorkflowRuntime + Send + Sync + 'static> SagaAdapterFactory<R> {
    /// Create a new factory with the given dependencies
    pub fn new(
        orchestrator: Option<Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>>,
        v4_runtime: Option<Arc<R>>,
    ) -> Self {
        Self {
            orchestrator,
            v4_runtime,
            default_adapter_type: SagaAdapterType::Legacy,
            v4_config: None,
        }
    }

    /// Set the default adapter type
    pub fn with_default_adapter_type(mut self, adapter_type: SagaAdapterType) -> Self {
        self.default_adapter_type = adapter_type;
        self
    }

    /// Set the v4 configuration
    pub fn with_v4_config(mut self, config: SagaEngineV4Config) -> Self {
        self.v4_config = Some(config);
        self
    }

    /// Create a legacy adapter for saga execution
    pub fn create_legacy_adapter<W: WorkflowDefinition>(&self) -> LegacySagaAdapter<W> {
        let orchestrator = self
            .orchestrator
            .as_ref()
            .expect("Orchestrator is required for legacy adapter");

        debug!("Creating legacy adapter");
        LegacySagaAdapter::new(orchestrator.clone())
    }

    /// Create a saga-engine v4.0 adapter for saga execution
    pub fn create_v4_adapter<W: WorkflowDefinition + Send + 'static>(
        &self,
    ) -> SagaEngineV4Adapter<R, W> {
        let runtime = self
            .v4_runtime
            .as_ref()
            .expect("V4 runtime is required for v4 adapter");

        debug!("Creating saga-engine v4 adapter");
        SagaEngineV4Adapter::new(runtime.clone(), self.v4_config.clone())
    }

    /// Create an adapter based on the configured type
    pub fn create_adapter<W: WorkflowDefinition + Send + 'static>(
        &self,
        adapter_type: Option<SagaAdapterType>,
    ) -> Box<dyn SagaPort<W, Error = std::io::Error> + Send + Sync> {
        let adapter_type = adapter_type.unwrap_or(self.default_adapter_type);

        match adapter_type {
            SagaAdapterType::Legacy => Box::new(self.create_legacy_adapter::<W>()),
            SagaAdapterType::V4 => Box::new(self.create_v4_adapter::<W>()),
        }
    }

    /// Check if legacy adapter is available
    pub fn is_legacy_available(&self) -> bool {
        self.orchestrator.is_some()
    }

    /// Check if v4 adapter is available
    pub fn is_v4_available(&self) -> bool {
        self.v4_runtime.is_some()
    }

    /// Get the default adapter type
    pub fn default_adapter_type(&self) -> SagaAdapterType {
        self.default_adapter_type
    }
}

/// Builder for SagaAdapterFactory
#[derive(Default, Clone)]
pub struct SagaAdapterFactoryBuilder<R: WorkflowRuntime + Send + Sync + 'static> {
    orchestrator: Option<Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>>,
    v4_runtime: Option<Arc<R>>,
    default_adapter_type: Option<SagaAdapterType>,
    v4_config: Option<SagaEngineV4Config>,
}

impl<R: WorkflowRuntime + Send + Sync + Default + 'static> SagaAdapterFactoryBuilder<R> {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the legacy orchestrator
    pub fn with_orchestrator(
        mut self,
        orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    /// Set the v4 runtime
    pub fn with_v4_runtime(mut self, runtime: Arc<R>) -> Self {
        self.v4_runtime = Some(runtime);
        self
    }

    /// Set the default adapter type
    pub fn with_default_adapter_type(mut self, adapter_type: SagaAdapterType) -> Self {
        self.default_adapter_type = Some(adapter_type);
        self
    }

    /// Set the v4 configuration
    pub fn with_v4_config(mut self, config: SagaEngineV4Config) -> Self {
        self.v4_config = Some(config);
        self
    }

    /// Build the factory
    pub fn build(self) -> SagaAdapterFactory<R> {
        let mut factory = SagaAdapterFactory::new(self.orchestrator, self.v4_runtime);

        if let Some(adapter_type) = self.default_adapter_type {
            factory = factory.with_default_adapter_type(adapter_type);
        }

        if let Some(config) = self.v4_config {
            factory = factory.with_v4_config(config);
        }

        factory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::adapters::v4_adapter::InMemoryWorkflowRuntime;
    use hodei_server_domain::saga::{Saga, SagaOrchestrator as DomainSagaOrchestrator};
    use saga_engine_core::workflow::{DynWorkflowStep, WorkflowConfig, WorkflowDefinition};

    struct TestSaga;

    impl Saga for TestSaga {
        fn saga_type(&self) -> SagaType {
            SagaType::Execution
        }

        fn steps(&self) -> Vec<Box<dyn hodei_server_domain::saga::SagaStep<Output = ()>>> {
            vec![]
        }
    }

    #[derive(Clone, Default)]
    struct MockOrchestrator;

    #[async_trait::async_trait]
    impl DomainSagaOrchestrator for MockOrchestrator {
        type Error = DomainError;

        async fn execute_saga(
            &self,
            _saga: &dyn Saga,
            _context: hodei_server_domain::saga::SagaContext,
        ) -> std::result::Result<hodei_server_domain::saga::SagaExecutionResult, Self::Error>
        {
            unreachable!()
        }

        async fn get_saga(
            &self,
            _saga_id: &hodei_server_domain::saga::SagaId,
        ) -> std::result::Result<Option<hodei_server_domain::saga::SagaContext>, Self::Error>
        {
            Ok(None)
        }

        async fn cancel_saga(
            &self,
            _saga_id: &hodei_server_domain::saga::SagaId,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn execute(
            &self,
            _context: &hodei_server_domain::saga::SagaContext,
        ) -> std::result::Result<hodei_server_domain::saga::SagaExecutionResult, Self::Error>
        {
            unreachable!()
        }
    }

    #[test]
    fn test_factory_creation() {
        let orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync> =
            Arc::new(MockOrchestrator);

        let factory: SagaAdapterFactory<InMemoryWorkflowRuntime> = SagaAdapterFactoryBuilder::new()
            .with_orchestrator(orchestrator)
            .with_default_adapter_type(SagaAdapterType::Legacy)
            .build();

        assert!(factory.is_legacy_available());
        assert!(!factory.is_v4_available());
        assert_eq!(factory.default_adapter_type(), SagaAdapterType::Legacy);
    }

    #[test]
    fn test_adapter_creation() {
        let orchestrator: Arc<dyn DomainSagaOrchestrator<Error = DomainError> + Send + Sync> =
            Arc::new(MockOrchestrator);

        let factory: SagaAdapterFactory<InMemoryWorkflowRuntime> = SagaAdapterFactoryBuilder::new()
            .with_orchestrator(orchestrator)
            .build();

        // Use a concrete workflow type
        struct TestWorkflow;
        impl WorkflowDefinition for TestWorkflow {
            const TYPE_ID: &'static str = "test";
            const VERSION: u32 = 1;
            fn steps(&self) -> &[Box<dyn DynWorkflowStep>] {
                &[]
            }
            fn configuration(&self) -> WorkflowConfig {
                WorkflowConfig::default()
            }
        }

        let _adapter = factory.create_legacy_adapter::<TestWorkflow>();
    }

    #[test]
    fn test_v4_runtime_creation() {
        let runtime = Arc::new(InMemoryWorkflowRuntime::default());

        let factory: SagaAdapterFactory<InMemoryWorkflowRuntime> = SagaAdapterFactoryBuilder::new()
            .with_v4_runtime(runtime)
            .with_default_adapter_type(SagaAdapterType::V4)
            .build();

        assert!(!factory.is_legacy_available());
        assert!(factory.is_v4_available());
        assert_eq!(factory.default_adapter_type(), SagaAdapterType::V4);
    }
}
