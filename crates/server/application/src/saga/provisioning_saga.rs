//! Provisioning Saga Coordinator - Application Layer
//!
//! Coordinates worker provisioning using the saga pattern with automatic compensation.

use crate::workers::provisioning::{ProvisioningResult, WorkerProvisioningService};
use hodei_server_domain::saga::{
    ProvisioningSaga, Saga, SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator,
};
use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::WorkerSpec;
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
    provisioning_service: Arc<dyn WorkerProvisioningService + Send + Sync>,
    config: ProvisioningSagaCoordinatorConfig,
}

impl DynProvisioningSagaCoordinator {
    /// Create a new DynProvisioningSagaCoordinator
    pub fn new(
        orchestrator: Arc<
            dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError>
                + Send
                + Sync,
        >,
        provisioning_service: Arc<dyn WorkerProvisioningService + Send + Sync>,
        config: Option<ProvisioningSagaCoordinatorConfig>,
    ) -> Self {
        Self {
            orchestrator,
            provisioning_service,
            config: config.unwrap_or_default(),
        }
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

        context
            .set_metadata("provider_id", &provider_id.to_string())
            .ok();
        if let Some(job_id) = job_id.clone() {
            context.set_metadata("job_id", &job_id.to_string()).ok();
        }
        context.set_metadata("worker_image", &spec.image).ok();

        let saga = ProvisioningSaga::new(spec.clone(), provider_id.clone());

        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(provider_id = %provider_id, "✅ Saga orchestration completed, provisioning worker");

                    // After saga completes, use provisioning_service to create the worker
                    match self
                        .provisioning_service
                        .provision_worker(provider_id, spec.clone())
                        .await
                    {
                        Ok(provisioning_result) => {
                            info!(
                                provider_id = %provider_id,
                                worker_id = %provisioning_result.worker_id,
                                "✅ Worker provisioned via saga"
                            );
                            Ok((provisioning_result.worker_id, result))
                        }
                        Err(e) => {
                            error!(provider_id = %provider_id, error = %e, "❌ Failed to provision worker");
                            Err(ProvisioningSagaError::SagaFailed {
                                message: format!("Worker provisioning failed: {}", e),
                            })
                        }
                    }
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
    provisioning_service: Option<Arc<dyn WorkerProvisioningService + Send + Sync>>,
    config: Option<ProvisioningSagaCoordinatorConfig>,
}

impl DynProvisioningSagaCoordinatorBuilder {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            provisioning_service: None,
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
        provisioning_service: Arc<dyn WorkerProvisioningService + Send + Sync>,
    ) -> Self {
        self.provisioning_service = Some(provisioning_service);
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

        Ok(DynProvisioningSagaCoordinator::new(
            orchestrator,
            provisioning_service,
            self.config,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DynProvisioningSagaCoordinatorBuilderError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

/// Provisioning Saga Coordinator
pub struct ProvisioningSagaCoordinator<OR>
where
    OR: SagaOrchestrator,
{
    orchestrator: Arc<OR>,
    provisioning_service: Arc<dyn WorkerProvisioningService + Send + Sync>,
    config: ProvisioningSagaCoordinatorConfig,
}

impl<OR> ProvisioningSagaCoordinator<OR>
where
    OR: SagaOrchestrator,
{
    pub fn new(
        orchestrator: Arc<OR>,
        provisioning_service: Arc<dyn WorkerProvisioningService + Send + Sync>,
        config: Option<ProvisioningSagaCoordinatorConfig>,
    ) -> Self {
        Self {
            orchestrator,
            provisioning_service,
            config: config.unwrap_or_default(),
        }
    }

    pub fn builder() -> ProvisioningSagaCoordinatorBuilder<OR> {
        ProvisioningSagaCoordinatorBuilder::new()
    }
}

impl<OR> ProvisioningSagaCoordinator<OR>
where
    OR: SagaOrchestrator + Send + Sync + 'static,
    OR::Error: std::fmt::Display + Send + Sync,
{
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

        context
            .set_metadata("provider_id", &provider_id.to_string())
            .ok();
        if let Some(job_id) = job_id.clone() {
            context.set_metadata("job_id", &job_id.to_string()).ok();
        }
        context.set_metadata("worker_image", &spec.image).ok();

        let saga = ProvisioningSaga::new(spec.clone(), provider_id.clone());

        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(provider_id = %provider_id, "✅ Saga orchestration completed, provisioning worker");

                    // After saga completes, use provisioning_service to create the worker
                    match self
                        .provisioning_service
                        .provision_worker(provider_id, spec.clone())
                        .await
                    {
                        Ok(provisioning_result) => {
                            info!(
                                provider_id = %provider_id,
                                worker_id = %provisioning_result.worker_id,
                                "✅ Worker provisioned via saga"
                            );
                            Ok((provisioning_result.worker_id, result))
                        }
                        Err(e) => {
                            error!(provider_id = %provider_id, error = %e, "❌ Failed to provision worker");
                            Err(ProvisioningSagaError::SagaFailed {
                                message: format!("Worker provisioning failed: {}", e),
                            })
                        }
                    }
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

/// Builder for ProvisioningSagaCoordinator
pub struct ProvisioningSagaCoordinatorBuilder<OR>
where
    OR: SagaOrchestrator,
{
    orchestrator: Option<Arc<OR>>,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService + Send + Sync>>,
    config: Option<ProvisioningSagaCoordinatorConfig>,
}

impl<OR: SagaOrchestrator> ProvisioningSagaCoordinatorBuilder<OR> {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            provisioning_service: None,
            config: None,
        }
    }

    pub fn with_orchestrator(mut self, orchestrator: Arc<OR>) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_provisioning_service(
        mut self,
        provisioning_service: Arc<dyn WorkerProvisioningService + Send + Sync>,
    ) -> Self {
        self.provisioning_service = Some(provisioning_service);
        self
    }

    pub fn with_config(mut self, config: ProvisioningSagaCoordinatorConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(
        self,
    ) -> Result<ProvisioningSagaCoordinator<OR>, ProvisioningSagaCoordinatorBuilderError> {
        let orchestrator = self
            .orchestrator
            .ok_or_else(|| ProvisioningSagaCoordinatorBuilderError::MissingField("orchestrator"))?;
        let provisioning_service = self.provisioning_service.ok_or_else(|| {
            ProvisioningSagaCoordinatorBuilderError::MissingField("provisioning_service")
        })?;

        Ok(ProvisioningSagaCoordinator::new(
            orchestrator,
            provisioning_service,
            self.config,
        ))
    }
}

#[derive(Debug, Error)]
pub enum ProvisioningSagaCoordinatorBuilderError {
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
        let result = ProvisioningSagaCoordinatorBuilder::<TestSagaOrchestrator>::new().build();
        assert!(result.is_err());
        match result {
            Err(ProvisioningSagaCoordinatorBuilderError::MissingField(_)) => {}
            _ => panic!("Expected MissingField error"),
        }
    }
}
