//! Provisioning Saga Coordinator - Application Layer
//!
//! Coordinates worker provisioning using the saga pattern with automatic compensation.

use hodei_server_domain::command::DynCommandBus;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::jobs::aggregate::JobRepositoryTx;
use hodei_server_domain::saga::{
    ProvisioningSaga, SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator, SagaServices,
};
use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::registry::WorkerRegistryTx;
use hodei_server_domain::workers::{WorkerProvisioning, WorkerSpec};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info, instrument, warn};

/// Configuration for provisioning saga coordinator
#[derive(Debug, Clone)]
pub struct ProvisioningSagaCoordinatorConfig {
    pub saga_timeout: Duration,
    pub step_timeout: Duration,
}

impl Default for ProvisioningSagaCoordinatorConfig {
    fn default() -> Self {
        Self {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Error)]
pub enum ProvisioningSagaError {
    #[error("Provider not found: {provider_id}")]
    ProviderNotFound { provider_id: ProviderId },
    #[error("Failed to provision worker: {message}")]
    ProvisioningFailed { message: String },
    #[error("Saga execution failed: {message}")]
    SagaFailed { message: String },
    #[error("Saga was compensated")]
    Compensated,
}

pub type ProvisioningSagaResult<T = ()> = std::result::Result<T, ProvisioningSagaError>;

/// Provisioning Saga Coordinator with trait objects (uses DomainError)
#[derive(Clone)]
pub struct DynProvisioningSagaCoordinator {
    orchestrator: Arc<
        dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError> + Send + Sync,
    >,
    /// Worker provisioning for saga steps
    provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync>,
    config: ProvisioningSagaCoordinatorConfig,
    /// Worker registry for SagaServices injection
    worker_registry: Arc<dyn WorkerRegistryTx + Send + Sync>,
    /// Event bus for SagaServices injection
    event_bus: Arc<dyn EventBus + Send + Sync>,
    /// Optional job repository for SagaServices injection
    job_repository: Option<Arc<dyn JobRepositoryTx + Send + Sync>>,
    /// CommandBus for saga steps (FIX GAP-60-01: Critical - must be injected)
    command_bus: DynCommandBus,
}

impl DynProvisioningSagaCoordinator {
    /// Create a new DynProvisioningSagaCoordinator with CommandBus
    ///
    /// # Arguments
    /// * `orchestrator` - The saga orchestrator to use
    /// * `provisioning_service` - The worker provisioning service
    /// * `worker_registry` - Worker registry for saga services
    /// * `event_bus` - Event bus for saga services
    /// * `job_repository` - Optional job repository for saga services
    /// * `command_bus` - CommandBus for saga command dispatch (GAP-60-01 fix)
    /// * `config` - Optional configuration
    pub fn new(
        orchestrator: Arc<
            dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError>
                + Send
                + Sync,
        >,
        provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistryTx + Send + Sync>,
        event_bus: Arc<dyn EventBus + Send + Sync>,
        job_repository: Option<Arc<dyn JobRepositoryTx + Send + Sync>>,
        command_bus: DynCommandBus,
        config: Option<ProvisioningSagaCoordinatorConfig>,
    ) -> Self {
        Self {
            orchestrator,
            provisioning_service,
            config: config.unwrap_or_default(),
            worker_registry,
            event_bus,
            job_repository,
            command_bus,
        }
    }

    /// Create a new builder for DynProvisioningSagaCoordinator
    pub fn builder() -> DynProvisioningSagaCoordinatorBuilder {
        DynProvisioningSagaCoordinatorBuilder::new()
    }

    #[instrument(skip(self), fields(provider_id = %provider_id, job_id = ?job_id), ret)]
    pub async fn execute_provisioning_saga(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> ProvisioningSagaResult<(WorkerId, SagaExecutionResult)> {
        info!(provider_id = %provider_id, "🛠️ Starting provisioning saga");

        let saga_id = SagaId::new();
        let saga_id_clone = saga_id.clone();
        let mut context = SagaContext::new(
            saga_id,
            hodei_server_domain::saga::SagaType::Provisioning,
            Some(format!("provisioning-{}", saga_id_clone.0)),
            Some("job_dispatcher".to_string()),
        );

        // EPIC-45 FIX: Inject SagaServices into context for CreateInfrastructureStep
        // GAP-60-01 FIX: Use with_command_bus to inject the CommandBus
        let provisioning_as_worker_provisioning: Arc<dyn WorkerProvisioning + Send + Sync> =
            self.provisioning_service.clone();

        let services = SagaServices::with_command_bus(
            self.worker_registry.clone(),
            self.event_bus.clone(),
            self.job_repository.clone(),
            Some(provisioning_as_worker_provisioning),
            None, // orchestrator
            self.command_bus.clone(),
        );
        context = context.with_services(Arc::new(services));
        info!(provider_id = %provider_id, "✅ SagaServices with CommandBus injected into context");

        context
            .set_metadata("provider_id", &provider_id.to_string())
            .ok();
        if let Some(job_id) = job_id.clone() {
            context.set_metadata("job_id", &job_id.to_string()).ok();
        }
        context.set_metadata("worker_image", &spec.image).ok();

        let saga = if let Some(jid) = job_id {
            ProvisioningSaga::with_job(spec.clone(), provider_id.clone(), jid)
        } else {
            ProvisioningSaga::new(spec.clone(), provider_id.clone())
        };

        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(provider_id = %provider_id, "✅ Saga orchestration completed successfully");

                    // EPIC-45 Gap 1: Extract worker_id from saga result metadata
                    // Get the saga context to access the metadata stored during execution
                    let saga_context = self
                        .orchestrator
                        .get_saga(&result.saga_id)
                        .await
                        .map_err(|e| ProvisioningSagaError::SagaFailed {
                            message: format!("Failed to get saga context: {}", e),
                        })?
                        .ok_or_else(|| ProvisioningSagaError::SagaFailed {
                            message: "Saga context not found".to_string(),
                        })?;

                    // The CreateInfrastructureStep already provisioned the worker
                    let worker_id_str = saga_context
                        .metadata
                        .get("worker_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ProvisioningSagaError::SagaFailed {
                            message: "No worker_id in saga result metadata".to_string(),
                        })?;

                    let worker_id = WorkerId::from_string(worker_id_str).ok_or_else(|| {
                        ProvisioningSagaError::SagaFailed {
                            message: "Invalid worker_id format in saga metadata".to_string(),
                        }
                    })?;

                    info!(
                        provider_id = %provider_id,
                        worker_id = %worker_id,
                        "✅ Worker provisioned via saga (compensation available)"
                    );
                    Ok((worker_id, result))
                } else if result.is_compensated() {
                    warn!(provider_id = %provider_id, "⚠️ Saga was compensated");
                    Err(ProvisioningSagaError::Compensated)
                } else {
                    error!(provider_id = %provider_id, "❌ Saga failed");
                    Err(ProvisioningSagaError::SagaFailed {
                        message: result
                            .error_message
                            .unwrap_or_else(|| "Unknown error".to_string()),
                    })
                }
            }
            Err(e) => {
                error!(provider_id = %provider_id, error = %e, "❌ Orchestrator error");
                Err(ProvisioningSagaError::SagaFailed {
                    message: e.to_string(),
                })
            }
        }
    }

    pub async fn get_saga_result(
        &self,
        saga_id: &SagaId,
    ) -> ProvisioningSagaResult<Option<SagaContext>> {
        self.orchestrator
            .get_saga(saga_id)
            .await
            .map_err(|e| ProvisioningSagaError::SagaFailed {
                message: e.to_string(),
            })
    }
}

/// Builder for DynProvisioningSagaCoordinator
#[derive(Clone)]
pub struct DynProvisioningSagaCoordinatorBuilder {
    orchestrator: Option<
        Arc<
            dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError>
                + Send
                + Sync,
        >,
    >,
    provisioning_service: Option<Arc<dyn WorkerProvisioning + Send + Sync>>,
    worker_registry: Option<Arc<dyn WorkerRegistryTx + Send + Sync>>,
    event_bus: Option<Arc<dyn EventBus + Send + Sync>>,
    job_repository: Option<Arc<dyn JobRepositoryTx + Send + Sync>>,
    /// CommandBus for saga steps (GAP-60-01 fix)
    command_bus: Option<DynCommandBus>,
    config: Option<ProvisioningSagaCoordinatorConfig>,
}

impl DynProvisioningSagaCoordinatorBuilder {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            provisioning_service: None,
            worker_registry: None,
            event_bus: None,
            job_repository: None,
            command_bus: None,
            config: None,
        }
    }

    pub fn with_orchestrator(
        mut self,
        orchestrator: Arc<
            dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError>
                + Send
                + Sync,
        >,
    ) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_provisioning_service(
        mut self,
        provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync>,
    ) -> Self {
        self.provisioning_service = Some(provisioning_service);
        self
    }

    pub fn with_worker_registry(
        mut self,
        worker_registry: Arc<dyn WorkerRegistryTx + Send + Sync>,
    ) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn JobRepositoryTx + Send + Sync>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    /// Set the CommandBus for saga steps (GAP-60-01 fix - REQUIRED)
    pub fn with_command_bus(mut self, command_bus: DynCommandBus) -> Self {
        self.command_bus = Some(command_bus);
        self
    }

    pub fn with_config(mut self, config: ProvisioningSagaCoordinatorConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(
        self,
    ) -> Result<DynProvisioningSagaCoordinator, DynProvisioningSagaCoordinatorBuilderError> {
        let orchestrator = self.orchestrator.ok_or_else(|| {
            DynProvisioningSagaCoordinatorBuilderError::MissingField("orchestrator")
        })?;
        let provisioning_service = self.provisioning_service.ok_or_else(|| {
            DynProvisioningSagaCoordinatorBuilderError::MissingField("provisioning_service")
        })?;
        let worker_registry = self.worker_registry.ok_or_else(|| {
            DynProvisioningSagaCoordinatorBuilderError::MissingField("worker_registry")
        })?;
        let event_bus = self
            .event_bus
            .ok_or_else(|| DynProvisioningSagaCoordinatorBuilderError::MissingField("event_bus"))?;
        // GAP-60-01: command_bus is now required
        let command_bus = self.command_bus.ok_or_else(|| {
            DynProvisioningSagaCoordinatorBuilderError::MissingField("command_bus")
        })?;

        Ok(DynProvisioningSagaCoordinator::new(
            orchestrator,
            provisioning_service,
            worker_registry,
            event_bus,
            self.job_repository,
            command_bus,
            self.config,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DynProvisioningSagaCoordinatorBuilderError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::SagaStep;
    use hodei_server_domain::saga::{Saga, SagaContext, SagaId, SagaType};
    use hodei_server_domain::shared_kernel::DomainError;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Clone, Debug)]
    struct TestSagaOrchestrator {
        pub should_fail: bool,
        pub executed_sagas: Arc<Mutex<Vec<(SagaId, String)>>>,
    }

    impl TestSagaOrchestrator {
        fn new() -> Self {
            Self {
                should_fail: false,
                executed_sagas: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn set_should_fail(&mut self, fail: bool) {
            self.should_fail = fail;
        }
    }

    #[async_trait::async_trait]
    impl SagaOrchestrator for TestSagaOrchestrator {
        type Error = DomainError;

        async fn execute_saga(
            &self,
            saga: &dyn Saga,
            context: SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            let saga_id = context.saga_id.clone();
            let saga_type = saga.saga_type().as_str().to_string();

            self.executed_sagas
                .lock()
                .await
                .push((saga_id.clone(), saga_type.clone()));

            if self.should_fail {
                return Ok(SagaExecutionResult::failed(
                    saga_id,
                    saga.saga_type(),
                    std::time::Duration::from_secs(1),
                    1,
                    0,
                    "Test failure".to_string(),
                ));
            }

            Ok(SagaExecutionResult::completed_with_steps(
                saga_id,
                saga.saga_type(),
                std::time::Duration::from_secs(1),
                4,
            ))
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

        /// EPIC-42: Execute saga directly from context (for reactive processing)
        async fn execute(
            &self,
            context: &SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            let saga_id = context.saga_id.clone();
            let saga_type = context.saga_type.as_str().to_string();

            self.executed_sagas
                .lock()
                .await
                .push((saga_id.clone(), saga_type.clone()));

            if self.should_fail {
                return Ok(SagaExecutionResult::failed(
                    saga_id,
                    context.saga_type,
                    std::time::Duration::from_secs(1),
                    1,
                    0,
                    "Test failure".to_string(),
                ));
            }

            Ok(SagaExecutionResult::completed_with_steps(
                saga_id,
                context.saga_type,
                std::time::Duration::from_secs(1),
                4,
            ))
        }
    }

    #[tokio::test]
    async fn test_config_defaults() {
        let config = ProvisioningSagaCoordinatorConfig::default();
        assert_eq!(config.saga_timeout, Duration::from_secs(300));
        assert_eq!(config.step_timeout, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_test_saga_orchestrator_success() {
        let orchestrator = TestSagaOrchestrator::new();

        struct SimpleTestSaga;
        impl Saga for SimpleTestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Provisioning
            }
            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![]
            }
        }

        let context = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        let saga = SimpleTestSaga;

        let result = orchestrator.execute_saga(&saga, context).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_success());
    }

    #[tokio::test]
    async fn test_test_saga_orchestrator_failure() {
        let mut orchestrator = TestSagaOrchestrator::new();
        orchestrator.set_should_fail(true);

        struct SimpleTestSaga;
        impl Saga for SimpleTestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Provisioning
            }
            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![]
            }
        }

        let context = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        let saga = SimpleTestSaga;

        let result = orchestrator.execute_saga(&saga, context).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_failed());
    }

    #[tokio::test]
    async fn test_builder_error() {
        let result = DynProvisioningSagaCoordinatorBuilder::new().build();
        assert!(result.is_err());
        match result {
            Err(DynProvisioningSagaCoordinatorBuilderError::MissingField(_)) => {}
            _ => panic!("Expected MissingField error"),
        }
    }
}
