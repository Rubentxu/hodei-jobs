//! Recovery Saga Coordinator - Application Layer
//!
//! Coordinates worker recovery using the saga pattern with automatic compensation.

use hodei_server_domain::command::{
    DynCommandBus, ErasedCommandBus, InMemoryErasedCommandBus,
};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::commands::provisioning::{
    CreateWorkerCommand, CreateWorkerHandler, DestroyWorkerCommand, DestroyWorkerHandler,
};
use hodei_server_domain::saga::{
    RecoverySaga, SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator, SagaServices,
};
use hodei_server_domain::shared_kernel::{DomainError, JobId, WorkerId};
use hodei_server_domain::workers::{WorkerProvisioning, WorkerRegistry};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info, warn};

/// Configuration for recovery saga coordinator
#[derive(Debug, Clone)]
pub struct RecoverySagaCoordinatorConfig {
    pub saga_timeout: Duration,
    pub step_timeout: Duration,
}

impl Default for RecoverySagaCoordinatorConfig {
    fn default() -> Self {
        Self {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Error)]
pub enum RecoverySagaError {
    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: WorkerId },
    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: JobId },
    #[error("Recovery failed: {message}")]
    RecoveryFailed { message: String },
    #[error("Saga execution failed: {message}")]
    SagaFailed { message: String },
    #[error("Saga was compensated")]
    Compensated,
}

pub type RecoverySagaResult<T = ()> = std::result::Result<T, RecoverySagaError>;

/// Recovery Saga Coordinator with trait objects (uses DomainError)
#[derive(Clone)]
pub struct DynRecoverySagaCoordinator {
    orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
    config: RecoverySagaCoordinatorConfig,
    /// Optional target provider for recovery (EPIC-46 GAP-05)
    pub target_provider_id: Option<String>,
    /// CommandBus for saga steps (FIX GAP-60-01: Critical - must be injected)
    command_bus: DynCommandBus,
    /// Worker registry for SagaServices injection
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    /// Event bus for SagaServices injection
    event_bus: Arc<dyn EventBus + Send + Sync>,
    /// Optional job repository for SagaServices injection
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    /// Optional worker provisioning for SagaServices injection
    provisioning_service: Option<Arc<dyn WorkerProvisioning + Send + Sync>>,
}

impl DynRecoverySagaCoordinator {
    /// Create a new DynRecoverySagaCoordinator with CommandBus
    pub fn new(
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
        command_bus: DynCommandBus,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn EventBus + Send + Sync>,
        job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
        provisioning_service: Option<Arc<dyn WorkerProvisioning + Send + Sync>>,
        config: Option<RecoverySagaCoordinatorConfig>,
        target_provider_id: Option<String>,
    ) -> Self {
        Self {
            orchestrator,
            config: config.unwrap_or_default(),
            target_provider_id,
            command_bus,
            worker_registry,
            event_bus,
            job_repository,
            provisioning_service,
        }
    }

    /// Create a new builder for DynRecoverySagaCoordinator
    pub fn builder() -> DynRecoverySagaCoordinatorBuilder {
        DynRecoverySagaCoordinatorBuilder::new()
    }

    /// Execute recovery saga for a failed worker and its job
    pub async fn execute_recovery_saga(
        &self,
        job_id: &JobId,
        failed_worker_id: &WorkerId,
    ) -> RecoverySagaResult<(SagaId, SagaExecutionResult)> {
        info!(
            job_id = %job_id,
            failed_worker_id = %failed_worker_id,
            "🔄 Starting recovery saga"
        );

        let saga_id = SagaId::new();
        let saga_id_for_return = saga_id.clone();
        let saga_id_for_context = saga_id.clone();
        let mut context = SagaContext::new(
            saga_id_for_context,
            hodei_server_domain::saga::SagaType::Recovery,
            Some(format!("recovery-{}", saga_id.0)),
            Some("lifecycle-manager".to_string()),
        );

        // GAP-60-01 FIX: Inject SagaServices with CommandBus into context
        let services = SagaServices::with_command_bus(
            self.worker_registry.clone(),
            self.event_bus.clone(),
            self.job_repository.clone(),
            self.provisioning_service.clone(),
            None, // orchestrator
            self.command_bus.clone(),
        );
        context = context.with_services(Arc::new(services));
        info!("✅ SagaServices with CommandBus injected into recovery context");

        context.set_metadata("job_id", &job_id.to_string()).ok();
        context
            .set_metadata("failed_worker_id", &failed_worker_id.to_string())
            .ok();

        // BUG-009 Fix: WorkerId is now correctly typed in RecoverySaga
        let saga = RecoverySaga::new(
            job_id.clone(),
            failed_worker_id.clone(),
            self.target_provider_id.clone(),
        );

        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(
                        job_id = %job_id,
                        "✅ Recovery saga completed successfully"
                    );
                    Ok((saga_id_for_return, result))
                } else if result.is_compensated() {
                    warn!(job_id = %job_id, "⚠️ Recovery saga was compensated");
                    Err(RecoverySagaError::Compensated)
                } else {
                    error!(job_id = %job_id, "❌ Recovery saga failed");
                    Err(RecoverySagaError::SagaFailed {
                        message: result
                            .error_message
                            .unwrap_or_else(|| "Unknown error".to_string()),
                    })
                }
            }
            Err(e) => {
                error!(job_id = %job_id, error = %e, "❌ Recovery orchestrator error");
                Err(RecoverySagaError::SagaFailed {
                    message: e.to_string(),
                })
            }
        }
    }

    pub async fn get_saga_result(
        &self,
        saga_id: &SagaId,
    ) -> RecoverySagaResult<Option<SagaContext>> {
        self.orchestrator
            .get_saga(saga_id)
            .await
            .map_err(|e| RecoverySagaError::SagaFailed {
                message: e.to_string(),
            })
    }
}

/// Builder for DynRecoverySagaCoordinator
#[derive(Clone)]
pub struct DynRecoverySagaCoordinatorBuilder {
    orchestrator: Option<Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>>,
    /// CommandBus for saga steps (GAP-60-01 fix)
    command_bus: Option<DynCommandBus>,
    /// Worker registry for SagaServices injection
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    /// Event bus for SagaServices injection
    event_bus: Option<Arc<dyn EventBus + Send + Sync>>,
    /// Optional job repository for SagaServices injection
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    /// Optional worker provisioning for SagaServices injection
    provisioning_service: Option<Arc<dyn WorkerProvisioning + Send + Sync>>,
    config: Option<RecoverySagaCoordinatorConfig>,
    target_provider_id: Option<String>,
}

impl DynRecoverySagaCoordinatorBuilder {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            command_bus: None,
            worker_registry: None,
            event_bus: None,
            job_repository: None,
            provisioning_service: None,
            config: None,
            target_provider_id: None,
        }
    }

    pub fn with_orchestrator(
        mut self,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    /// Set the CommandBus for saga steps (GAP-60-01 fix - REQUIRED)
    pub fn with_command_bus(mut self, command_bus: DynCommandBus) -> Self {
        self.command_bus = Some(command_bus);
        self
    }

    /// Set the worker registry for saga services
    pub fn with_worker_registry(
        mut self,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    /// Set the event bus for saga services
    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Set the job repository for saga services
    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    /// Set the worker provisioning service for saga services
    pub fn with_provisioning_service(
        mut self,
        provisioning_service: Arc<dyn WorkerProvisioning + Send + Sync>,
    ) -> Self {
        self.provisioning_service = Some(provisioning_service);
        self
    }

    pub fn with_config(mut self, config: RecoverySagaCoordinatorConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_target_provider_id(mut self, provider_id: String) -> Self {
        self.target_provider_id = Some(provider_id);
        self
    }

    pub fn build(
        self,
    ) -> std::result::Result<DynRecoverySagaCoordinator, DynRecoverySagaCoordinatorBuilderError>
    {
        let orchestrator = self.orchestrator.ok_or_else(|| {
            DynRecoverySagaCoordinatorBuilderError::MissingField("orchestrator")
        })?;
        // GAP-60-01: command_bus is now required
        let command_bus = self.command_bus.ok_or_else(|| {
            DynRecoverySagaCoordinatorBuilderError::MissingField("command_bus")
        })?;
        let worker_registry = self.worker_registry.ok_or_else(|| {
            DynRecoverySagaCoordinatorBuilderError::MissingField("worker_registry")
        })?;
        let event_bus = self.event_bus.ok_or_else(|| {
            DynRecoverySagaCoordinatorBuilderError::MissingField("event_bus")
        })?;

        Ok(DynRecoverySagaCoordinator::new(
            orchestrator,
            command_bus,
            worker_registry,
            event_bus,
            self.job_repository,
            self.provisioning_service,
            self.config,
            self.target_provider_id,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DynRecoverySagaCoordinatorBuilderError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

/// Recovery Saga Coordinator (generic version)
pub struct RecoverySagaCoordinator<OR>
where
    OR: SagaOrchestrator,
{
    orchestrator: Arc<OR>,
    config: RecoverySagaCoordinatorConfig,
    /// Optional target provider for recovery (EPIC-46 GAP-05)
    pub target_provider_id: Option<String>,
}

impl<OR> RecoverySagaCoordinator<OR>
where
    OR: SagaOrchestrator,
{
    pub fn new(
        orchestrator: Arc<OR>,
        config: Option<RecoverySagaCoordinatorConfig>,
        target_provider_id: Option<String>,
    ) -> Self {
        Self {
            orchestrator,
            config: config.unwrap_or_default(),
            target_provider_id,
        }
    }

    pub fn builder() -> RecoverySagaCoordinatorBuilder<OR> {
        RecoverySagaCoordinatorBuilder::new()
    }
}

impl<OR> RecoverySagaCoordinator<OR>
where
    OR: SagaOrchestrator + Send + Sync + 'static,
    OR::Error: std::fmt::Display + Send + Sync,
{
    pub async fn execute_recovery_saga(
        &self,
        job_id: &JobId,
        failed_worker_id: &WorkerId,
    ) -> RecoverySagaResult<(SagaId, SagaExecutionResult)> {
        info!(
            job_id = %job_id,
            failed_worker_id = %failed_worker_id,
            "🔄 Starting recovery saga"
        );

        let saga_id = SagaId::new();
        let saga_id_for_return = saga_id.clone();
        let saga_id_for_context = saga_id.clone();
        let mut context = SagaContext::new(
            saga_id_for_context,
            hodei_server_domain::saga::SagaType::Recovery,
            Some(format!("recovery-{}", saga_id.0)),
            Some("lifecycle-manager".to_string()),
        );

        context.set_metadata("job_id", &job_id.to_string()).ok();
        context
            .set_metadata("failed_worker_id", &failed_worker_id.to_string())
            .ok();

        // BUG-009 Fix: WorkerId is now correctly typed in RecoverySaga
        let saga = RecoverySaga::new(
            job_id.clone(),
            failed_worker_id.clone(),
            self.target_provider_id.clone(),
        );

        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(
                        job_id = %job_id,
                        "✅ Recovery saga completed successfully"
                    );
                    Ok((saga_id_for_return, result))
                } else if result.is_compensated() {
                    warn!(job_id = %job_id, "⚠️ Recovery saga was compensated");
                    Err(RecoverySagaError::Compensated)
                } else {
                    error!(job_id = %job_id, "❌ Recovery saga failed");
                    Err(RecoverySagaError::SagaFailed {
                        message: result
                            .error_message
                            .unwrap_or_else(|| "Unknown error".to_string()),
                    })
                }
            }
            Err(e) => {
                error!(job_id = %job_id, error = %e, "❌ Recovery orchestrator error");
                Err(RecoverySagaError::SagaFailed {
                    message: e.to_string(),
                })
            }
        }
    }

    pub async fn get_saga_result(
        &self,
        saga_id: &SagaId,
    ) -> RecoverySagaResult<Option<SagaContext>> {
        self.orchestrator
            .get_saga(saga_id)
            .await
            .map_err(|e| RecoverySagaError::SagaFailed {
                message: e.to_string(),
            })
    }
}

/// Builder for RecoverySagaCoordinator
pub struct RecoverySagaCoordinatorBuilder<OR>
where
    OR: SagaOrchestrator,
{
    orchestrator: Option<Arc<OR>>,
    config: Option<RecoverySagaCoordinatorConfig>,
    target_provider_id: Option<String>,
}

impl<OR: SagaOrchestrator> RecoverySagaCoordinatorBuilder<OR> {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            config: None,
            target_provider_id: None,
        }
    }

    pub fn with_orchestrator(mut self, orchestrator: Arc<OR>) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_config(mut self, config: RecoverySagaCoordinatorConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_target_provider_id(mut self, provider_id: String) -> Self {
        self.target_provider_id = Some(provider_id);
        self
    }

    pub fn build(
        self,
    ) -> std::result::Result<RecoverySagaCoordinator<OR>, RecoverySagaCoordinatorBuilderError> {
        let orchestrator = self
            .orchestrator
            .ok_or_else(|| RecoverySagaCoordinatorBuilderError::MissingField("orchestrator"))?;

        Ok(RecoverySagaCoordinator::new(
            orchestrator,
            self.config,
            self.target_provider_id,
        ))
    }
}

#[derive(Debug, Error)]
pub enum RecoverySagaCoordinatorBuilderError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::{Saga, SagaContext, SagaId, SagaType};
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
                5,
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
                5,
            ))
        }
    }

    #[tokio::test]
    async fn test_recovery_config_defaults() {
        let config = RecoverySagaCoordinatorConfig::default();
        assert_eq!(config.saga_timeout, Duration::from_secs(300));
        assert_eq!(config.step_timeout, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_recovery_saga_execution_success() {
        let orchestrator = Arc::new(TestSagaOrchestrator::new());
        let coordinator = DynRecoverySagaCoordinator::new(orchestrator, None, None);

        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        let result = coordinator.execute_recovery_saga(&job_id, &worker_id).await;

        assert!(result.is_ok());
        let (saga_id, saga_result) = result.unwrap();
        assert!(saga_result.is_success());
    }

    #[tokio::test]
    async fn test_recovery_saga_execution_failure() {
        let mut orchestrator = TestSagaOrchestrator::new();
        orchestrator.set_should_fail(true);
        let orchestrator = Arc::new(orchestrator);

        let coordinator = DynRecoverySagaCoordinator::new(orchestrator, None, None);

        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        let result = coordinator.execute_recovery_saga(&job_id, &worker_id).await;

        // When saga fails, the coordinator returns an error
        assert!(result.is_err());
        match result {
            Err(RecoverySagaError::SagaFailed { message }) => {
                assert_eq!(message, "Test failure");
            }
            _ => panic!("Expected SagaFailed error"),
        }
    }

    #[tokio::test]
    async fn test_builder_error() {
        let result = DynRecoverySagaCoordinatorBuilder::new().build();
        assert!(result.is_err());
        match result {
            Err(DynRecoverySagaCoordinatorBuilderError::MissingField(_)) => {}
            _ => panic!("Expected MissingField error"),
        }
    }
}
