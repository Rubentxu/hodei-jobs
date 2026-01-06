//! Recovery Saga Coordinator - Application Layer
//!
//! Coordinates worker recovery using the saga pattern with automatic compensation.

use hodei_server_domain::saga::{
    RecoverySaga, SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator,
};
use hodei_server_domain::shared_kernel::{DomainError, JobId, WorkerId};
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
}

impl DynRecoverySagaCoordinator {
    /// Create a new DynRecoverySagaCoordinator
    pub fn new(
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
        config: Option<RecoverySagaCoordinatorConfig>,
        target_provider_id: Option<String>,
    ) -> Self {
        Self {
            orchestrator,
            config: config.unwrap_or_default(),
            target_provider_id,
        }
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
    config: Option<RecoverySagaCoordinatorConfig>,
}

impl DynRecoverySagaCoordinatorBuilder {
    pub fn new() -> Self {
        Self {
            orchestrator: None,
            config: None,
        }
    }

    pub fn with_orchestrator(
        mut self,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_config(mut self, config: RecoverySagaCoordinatorConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(
        self,
    ) -> std::result::Result<DynRecoverySagaCoordinator, DynRecoverySagaCoordinatorBuilderError>
    {
        let orchestrator = self
            .orchestrator
            .ok_or_else(|| DynRecoverySagaCoordinatorBuilderError::MissingField("orchestrator"))?;

        Ok(DynRecoverySagaCoordinator::new(
            orchestrator,
            self.config,
            None,
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
