//! Execution Saga Dispatcher - Application Layer
//!
//! Implements the ExecutionSaga coordination with JobDispatcher using saga pattern.

use hodei_server_domain::command::DynCommandBus;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::{
    ExecutionSaga, SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator, SagaServices,
};
use hodei_server_domain::shared_kernel::{JobId, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info, warn};

/// Configuration for execution saga dispatcher
#[derive(Debug, Clone)]
pub struct ExecutionSagaDispatcherConfig {
    pub saga_timeout: Duration,
    pub step_timeout: Duration,
}

impl Default for ExecutionSagaDispatcherConfig {
    fn default() -> Self {
        Self {
            saga_timeout: Duration::from_secs(60),
            step_timeout: Duration::from_secs(30),
        }
    }
}

/// Errors from execution saga dispatcher
#[derive(Debug, Error)]
pub enum ExecutionSagaError {
    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: JobId },
    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: String },
    #[error("Saga execution failed: {message}")]
    SagaFailed { message: String },
    #[error("Saga was compensated")]
    Compensated,
}

pub type ExecutionSagaResult<T = ()> = std::result::Result<T, ExecutionSagaError>;

/// Execution Saga Dispatcher with trait objects (uses DomainError)
#[derive(Clone)]
pub struct DynExecutionSagaDispatcher {
    orchestrator: Arc<
        dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError> + Send + Sync,
    >,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    event_bus: Arc<dyn EventBus + Send + Sync>,
    command_bus: DynCommandBus,
    config: ExecutionSagaDispatcherConfig,
}

impl DynExecutionSagaDispatcher {
    /// Create a new DynExecutionSagaDispatcher
    pub fn new(
        orchestrator: Arc<
            dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError>
                + Send
                + Sync,
        >,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn EventBus + Send + Sync>,
        command_bus: DynCommandBus,
        config: Option<ExecutionSagaDispatcherConfig>,
    ) -> Self {
        Self {
            orchestrator,
            job_repository,
            worker_registry,
            event_bus,
            command_bus,
            config: config.unwrap_or_default(),
        }
    }

    pub fn builder() -> DynExecutionSagaDispatcherBuilder {
        DynExecutionSagaDispatcherBuilder::new()
    }

    pub async fn execute_execution_saga(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> ExecutionSagaResult<SagaExecutionResult> {
        info!(job_id = %job_id, worker_id = %worker_id, "�� Starting execution saga");

        let _job = self
            .job_repository
            .find_by_id(job_id)
            .await
            .map_err(|e| ExecutionSagaError::SagaFailed {
                message: format!("Failed to fetch job: {}", e),
            })?
            .ok_or_else(|| ExecutionSagaError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        let _worker = self
            .worker_registry
            .get(worker_id)
            .await
            .map_err(|e| ExecutionSagaError::SagaFailed {
                message: format!("Failed to fetch worker: {}", e),
            })?
            .ok_or_else(|| ExecutionSagaError::WorkerNotFound {
                worker_id: worker_id.to_string(),
            })?;

        // Deterministic saga_id for idempotency (same job+worker = same saga)
        let saga_id = SagaId::from_string(&format!("execution-{}-{}", job_id.0, worker_id.0));
        let mut context = SagaContext::new(
            saga_id,
            hodei_server_domain::saga::SagaType::Execution,
            Some(format!("job-{}", job_id.0)),
            Some("job_dispatcher".to_string()),
        );

        // Inject SagaServices into context
        let services = SagaServices::with_command_bus(
            self.worker_registry.clone(),
            self.event_bus.clone(),
            Some(self.job_repository.clone()),
            None, // provisioning
            None, // orchestrator
            self.command_bus.clone(),
        );
        context = context.with_services(Arc::new(services));

        context.set_metadata("job_id", &job_id.to_string()).ok();
        context
            .set_metadata("worker_id", &worker_id.to_string())
            .ok();

        // Use with_worker to pass the pre-assigned worker_id to the saga
        let saga = ExecutionSaga::with_worker(job_id.clone(), worker_id.clone());

        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(job_id = %job_id, "✅ Saga completed successfully");
                    Ok(result)
                } else if result.is_compensated() {
                    warn!(job_id = %job_id, "⚠️ Saga was compensated");
                    Err(ExecutionSagaError::Compensated)
                } else {
                    error!(job_id = %job_id, "❌ Saga failed");
                    Err(ExecutionSagaError::SagaFailed {
                        message: result
                            .error_message
                            .unwrap_or_else(|| "Unknown error".to_string()),
                    })
                }
            }
            Err(e) => {
                error!(job_id = %job_id, error = %e, "❌ Orchestrator error");
                Err(ExecutionSagaError::SagaFailed {
                    message: e.to_string(),
                })
            }
        }
    }

    pub async fn get_saga_result(
        &self,
        saga_id: &SagaId,
    ) -> ExecutionSagaResult<Option<SagaContext>> {
        self.orchestrator
            .get_saga(saga_id)
            .await
            .map_err(|e| ExecutionSagaError::SagaFailed {
                message: e.to_string(),
            })
    }
}

/// Builder for DynExecutionSagaDispatcher
#[derive(Clone)]
pub struct DynExecutionSagaDispatcherBuilder {
    orchestrator: Option<
        Arc<
            dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError>
                + Send
                + Sync,
        >,
    >,
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    event_bus: Option<Arc<dyn EventBus + Send + Sync>>,
    command_bus: Option<DynCommandBus>,
    config: Option<ExecutionSagaDispatcherConfig>,
}

impl DynExecutionSagaDispatcherBuilder {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            job_repository: None,
            worker_registry: None,
            event_bus: None,
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

    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    pub fn with_worker_registry(
        mut self,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_command_bus(mut self, command_bus: DynCommandBus) -> Self {
        self.command_bus = Some(command_bus);
        self
    }

    pub fn with_config(mut self, config: ExecutionSagaDispatcherConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(
        self,
    ) -> Result<DynExecutionSagaDispatcher, DynExecutionSagaDispatcherBuilderError> {
        let orchestrator = self
            .orchestrator
            .ok_or_else(|| DynExecutionSagaDispatcherBuilderError::MissingField("orchestrator"))?;
        let job_repository = self.job_repository.ok_or_else(|| {
            DynExecutionSagaDispatcherBuilderError::MissingField("job_repository")
        })?;
        let worker_registry = self.worker_registry.ok_or_else(|| {
            DynExecutionSagaDispatcherBuilderError::MissingField("worker_registry")
        })?;
        let event_bus = self
            .event_bus
            .ok_or_else(|| DynExecutionSagaDispatcherBuilderError::MissingField("event_bus"))?;
        let command_bus = self
            .command_bus
            .ok_or_else(|| DynExecutionSagaDispatcherBuilderError::MissingField("command_bus"))?;

        Ok(DynExecutionSagaDispatcher::new(
            orchestrator,
            job_repository,
            worker_registry,
            event_bus,
            command_bus,
            self.config,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DynExecutionSagaDispatcherBuilderError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

/// Execution Saga Dispatcher
pub struct ExecutionSagaDispatcher<OR>
where
    OR: SagaOrchestrator,
{
    orchestrator: Arc<OR>,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    event_bus: Arc<dyn EventBus + Send + Sync>,
    command_bus: DynCommandBus,
    config: ExecutionSagaDispatcherConfig,
}

impl<OR> ExecutionSagaDispatcher<OR>
where
    OR: SagaOrchestrator,
{
    pub fn new(
        orchestrator: Arc<OR>,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn EventBus + Send + Sync>,
        command_bus: DynCommandBus,
        config: Option<ExecutionSagaDispatcherConfig>,
    ) -> Self {
        Self {
            orchestrator,
            job_repository,
            worker_registry,
            event_bus,
            command_bus,
            config: config.unwrap_or_default(),
        }
    }

    pub fn builder() -> ExecutionSagaDispatcherBuilder<OR> {
        ExecutionSagaDispatcherBuilder::new()
    }
}

impl<OR> ExecutionSagaDispatcher<OR>
where
    OR: SagaOrchestrator + Send + Sync + 'static,
    OR::Error: std::fmt::Display + Send + Sync,
{
    pub async fn execute_execution_saga(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> ExecutionSagaResult<SagaExecutionResult> {
        info!(job_id = %job_id, worker_id = %worker_id, "📦 Starting execution saga");

        let _job = self
            .job_repository
            .find_by_id(job_id)
            .await
            .map_err(|e| ExecutionSagaError::SagaFailed {
                message: format!("Failed to fetch job: {}", e),
            })?
            .ok_or_else(|| ExecutionSagaError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        let _worker = self
            .worker_registry
            .get(worker_id)
            .await
            .map_err(|e| ExecutionSagaError::SagaFailed {
                message: format!("Failed to fetch worker: {}", e),
            })?
            .ok_or_else(|| ExecutionSagaError::WorkerNotFound {
                worker_id: worker_id.to_string(),
            })?;

        // Deterministic saga_id for idempotency (same job+worker = same saga)
        let saga_id = SagaId::from_string(&format!("execution-{}-{}", job_id.0, worker_id.0));
        let mut context = SagaContext::new(
            saga_id,
            hodei_server_domain::saga::SagaType::Execution,
            Some(format!("job-{}", job_id.0)),
            Some("job_dispatcher".to_string()),
        );

        // Inject SagaServices into context
        let services = SagaServices::with_command_bus(
            self.worker_registry.clone(),
            self.event_bus.clone(),
            Some(self.job_repository.clone()),
            None, // provisioning
            None, // orchestrator
            self.command_bus.clone(),
        );
        context = context.with_services(Arc::new(services));

        context.set_metadata("job_id", &job_id.to_string()).ok();
        context
            .set_metadata("worker_id", &worker_id.to_string())
            .ok();

        let saga = ExecutionSaga::new(job_id.clone());

        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(job_id = %job_id, "✅ Saga completed successfully");
                    Ok(result)
                } else if result.is_compensated() {
                    warn!(job_id = %job_id, "⚠️ Saga was compensated");
                    Err(ExecutionSagaError::Compensated)
                } else {
                    error!(job_id = %job_id, "❌ Saga failed");
                    Err(ExecutionSagaError::SagaFailed {
                        message: result
                            .error_message
                            .unwrap_or_else(|| "Unknown error".to_string()),
                    })
                }
            }
            Err(e) => {
                error!(job_id = %job_id, error = %e, "❌ Orchestrator error");
                Err(ExecutionSagaError::SagaFailed {
                    message: e.to_string(),
                })
            }
        }
    }

    pub async fn get_saga_result(
        &self,
        saga_id: &SagaId,
    ) -> ExecutionSagaResult<Option<SagaContext>> {
        self.orchestrator
            .get_saga(saga_id)
            .await
            .map_err(|e| ExecutionSagaError::SagaFailed {
                message: e.to_string(),
            })
    }
}

/// Builder for ExecutionSagaDispatcher
pub struct ExecutionSagaDispatcherBuilder<OR>
where
    OR: SagaOrchestrator,
{
    orchestrator: Option<Arc<OR>>,
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    event_bus: Option<Arc<dyn EventBus + Send + Sync>>,
    command_bus: Option<DynCommandBus>,
    config: Option<ExecutionSagaDispatcherConfig>,
}

impl<OR: SagaOrchestrator> ExecutionSagaDispatcherBuilder<OR> {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            job_repository: None,
            worker_registry: None,
            event_bus: None,
            command_bus: None,
            config: None,
        }
    }

    pub fn with_orchestrator(mut self, orchestrator: Arc<OR>) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    pub fn with_worker_registry(
        mut self,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_command_bus(mut self, command_bus: DynCommandBus) -> Self {
        self.command_bus = Some(command_bus);
        self
    }

    pub fn with_config(mut self, config: ExecutionSagaDispatcherConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> Result<ExecutionSagaDispatcher<OR>, ExecutionSagaDispatcherBuilderError> {
        let orchestrator = self
            .orchestrator
            .ok_or_else(|| ExecutionSagaDispatcherBuilderError::MissingField("orchestrator"))?;
        let job_repository = self
            .job_repository
            .ok_or_else(|| ExecutionSagaDispatcherBuilderError::MissingField("job_repository"))?;
        let worker_registry = self
            .worker_registry
            .ok_or_else(|| ExecutionSagaDispatcherBuilderError::MissingField("worker_registry"))?;
        let event_bus = self
            .event_bus
            .ok_or_else(|| ExecutionSagaDispatcherBuilderError::MissingField("event_bus"))?;
        let command_bus = self
            .command_bus
            .ok_or_else(|| ExecutionSagaDispatcherBuilderError::MissingField("command_bus"))?;

        Ok(ExecutionSagaDispatcher::new(
            orchestrator,
            job_repository,
            worker_registry,
            event_bus,
            command_bus,
            self.config,
        ))
    }
}

#[derive(Debug, Error)]
pub enum ExecutionSagaDispatcherBuilderError {
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
        let config = ExecutionSagaDispatcherConfig::default();
        assert_eq!(config.saga_timeout, Duration::from_secs(60));
        assert_eq!(config.step_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_test_saga_orchestrator_success() {
        let orchestrator = TestSagaOrchestrator::new();

        struct SimpleTestSaga;
        impl Saga for SimpleTestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }
            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![]
            }
        }

        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
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
                SagaType::Execution
            }
            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![]
            }
        }

        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        let saga = SimpleTestSaga;

        let result = orchestrator.execute_saga(&saga, context).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_failed());
    }

    #[tokio::test]
    async fn test_builder_error() {
        let result = ExecutionSagaDispatcherBuilder::<TestSagaOrchestrator>::new().build();
        assert!(result.is_err());
        match result {
            Err(ExecutionSagaDispatcherBuilderError::MissingField(_)) => {}
            _ => panic!("Expected MissingField error"),
        }
    }
}
