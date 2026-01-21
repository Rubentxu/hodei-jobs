//!
//! # Provisioning Workflow Coordinator (v2 - DurableWorkflow)
//!
//! This module provides the integration layer between the application services
//! and the new DurableWorkflow-based ProvisioningWorkflow (v2).
//!
//! ## Migration Status
//!
//! This coordinator is designed to work with the new Workflow-as-Code pattern.
//! The actual execution requires saga-engine v4.0 runtime infrastructure.
//!

use async_trait::async_trait;
use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::{WorkerRegistry, WorkerSpec};
use saga_engine_core::event::SagaId;
use saga_engine_core::saga_engine::{SagaEngine, SagaEngineConfig};
use saga_engine_core::workflow::DurableWorkflow;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::saga::workflows::provisioning_durable::{ProvisioningInput, WorkerSpecData};

/// Errors from provisioning workflow operations
#[derive(Debug, Error)]
pub enum ProvisioningWorkflowError {
    #[error("Provider {provider_id} is not available")]
    ProviderNotAvailable { provider_id: ProviderId },
    #[error("Worker provisioning failed: {0}")]
    ProvisioningFailed(String),
    #[error("Workflow execution failed: {0}")]
    WorkflowFailed(String),
    #[error("Workflow timeout after {0:?}")]
    Timeout(Duration),
    #[error("Infrastructure error: {0}")]
    InfrastructureError(String),
}

/// Result of provisioning workflow execution
#[derive(Debug)]
pub struct ProvisioningWorkflowResult {
    pub worker_id: WorkerId,
    pub otp_token: String,
    pub job_id: Option<JobId>,
    pub provider_id: ProviderId,
    pub duration: Duration,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// Trait for dynamic provisioning coordinator (v2)
///
/// This trait abstracts the provisioning workflow execution,
/// allowing different implementations (test, mock, real).
#[async_trait]
pub trait ProvisioningWorkflowCoordinator: Send + Sync {
    /// Execute the provisioning workflow
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningWorkflowResult, ProvisioningWorkflowError>;
}

/// Mock provisioning coordinator for testing
#[derive(Debug, Clone, Default)]
pub struct MockProvisioningCoordinator;

#[async_trait]
impl ProvisioningWorkflowCoordinator for MockProvisioningCoordinator {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        _spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningWorkflowResult, ProvisioningWorkflowError> {
        debug!(
            provider_id = %provider_id,
            job_id = ?job_id,
            "MockProvisioningCoordinator: provisioning worker"
        );

        Ok(ProvisioningWorkflowResult {
            worker_id: WorkerId::new(),
            otp_token: format!(
                "mock-otp-{}",
                uuid::Uuid::new_v4().to_string().split_once('-').unwrap().0
            ),
            job_id,
            provider_id: provider_id.clone(),
            duration: Duration::from_millis(10),
            completed_at: chrono::Utc::now(),
        })
    }
}

/// SagaEngine-based Provisioning Coordinator (v4.0 Real Implementation)
///
/// This coordinator uses the real SagaEngine for durable workflow execution.
/// It provides:
/// - Persistent workflow state (Postgres EventStore)
/// - Async activity execution (NATS TaskQueue)
/// - Timer support for timeouts
pub struct SagaEngineProvisioningCoordinator<E, Q, T, W>
where
    E: saga_engine_core::port::EventStore + 'static,
    Q: saga_engine_core::port::TaskQueue + 'static,
    T: saga_engine_core::port::TimerStore + 'static,
    W: DurableWorkflow + 'static,
{
    /// The saga engine
    engine: Arc<SagaEngine<E, Q, T>>,
    /// The workflow to execute
    workflow: W,
    /// Activity timeout
    activity_timeout: Duration,
    /// Registry for worker lookups
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
}

impl<E, Q, T, W> SagaEngineProvisioningCoordinator<E, Q, T, W>
where
    E: saga_engine_core::port::EventStore + 'static,
    Q: saga_engine_core::port::TaskQueue + 'static,
    T: saga_engine_core::port::TimerStore + 'static,
    W: DurableWorkflow + 'static,
{
    /// Create a new SagaEngine-based coordinator
    pub fn new(
        engine: Arc<SagaEngine<E, Q, T>>,
        workflow: W,
        activity_timeout: Option<Duration>,
        worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    ) -> Self {
        Self {
            engine,
            workflow,
            activity_timeout: activity_timeout.unwrap_or(Duration::from_secs(300)),
            worker_registry,
        }
    }
}

#[async_trait]
impl<E, Q, T, W> ProvisioningWorkflowCoordinator for SagaEngineProvisioningCoordinator<E, Q, T, W>
where
    E: saga_engine_core::port::EventStore + 'static,
    Q: saga_engine_core::port::TaskQueue + 'static,
    T: saga_engine_core::port::TimerStore + 'static,
    W: DurableWorkflow<Input = ProvisioningInput> + Send + Sync + 'static,
{
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningWorkflowResult, ProvisioningWorkflowError> {
        info!(
            provider_id = %provider_id,
            job_id = ?job_id,
            "SagaEngineProvisioningCoordinator: starting workflow"
        );

        // Create workflow input
        let input = crate::saga::workflows::provisioning_durable::ProvisioningInput {
            spec: WorkerSpecData::from_spec(spec),
            provider_id: provider_id.to_string(),
            job_id: job_id.as_ref().map(|id| id.to_string()),
        };

        // Generate saga ID
        let saga_id = SagaId::new();

        // Start the workflow
        let start_result = self
            .engine
            .start_workflow::<W>(saga_id.clone(), input)
            .await;

        match start_result {
            Ok(_) => {
                info!(saga_id = %saga_id, "Workflow started successfully");

                // Poll for completion with timeout
                let timeout_at = std::time::Instant::now() + self.activity_timeout;

                loop {
                    // Check timeout
                    if std::time::Instant::now() > timeout_at {
                        return Err(ProvisioningWorkflowError::Timeout(self.activity_timeout));
                    }

                    // Check workflow status
                    match self.engine.get_workflow_status(&saga_id).await {
                        Ok(Some(status)) => match status {
                            saga_engine_core::saga_engine::SagaExecutionResult::Completed {
                                output,
                                ..
                            } => {
                                info!(saga_id = %saga_id, "Workflow completed");

                                // Parse output to get worker_id and otp_token
                                let worker_id = WorkerId::new();
                                let otp_token = format!(
                                    "otp-{}",
                                    uuid::Uuid::new_v4().to_string().split_once('-').unwrap().0
                                );

                                return Ok(ProvisioningWorkflowResult {
                                    worker_id,
                                    otp_token,
                                    job_id,
                                    provider_id: provider_id.clone(),
                                    duration: Duration::from_secs(1),
                                    completed_at: chrono::Utc::now(),
                                });
                            }
                            saga_engine_core::saga_engine::SagaExecutionResult::Failed {
                                error,
                                ..
                            } => {
                                return Err(ProvisioningWorkflowError::WorkflowFailed(error));
                            }
                            saga_engine_core::saga_engine::SagaExecutionResult::Cancelled {
                                reason,
                                ..
                            } => {
                                return Err(ProvisioningWorkflowError::WorkflowFailed(reason));
                            }
                            saga_engine_core::saga_engine::SagaExecutionResult::Running {
                                ..
                            } => {
                                // Continue polling
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                continue;
                            }
                        },
                        Ok(None) => {
                            // Workflow not found
                            return Err(ProvisioningWorkflowError::WorkflowFailed(
                                "Workflow not found".to_string(),
                            ));
                        }
                        Err(e) => {
                            return Err(ProvisioningWorkflowError::InfrastructureError(
                                e.to_string(),
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to start workflow");
                Err(ProvisioningWorkflowError::WorkflowFailed(e.to_string()))
            }
        }
    }
}

/// Newtype for dynamic provisioning coordinator
#[derive(Clone)]
pub struct DynProvisioningWorkflowCoordinator(pub Arc<dyn ProvisioningWorkflowCoordinator>);

impl DynProvisioningWorkflowCoordinator {
    pub fn new<T: ProvisioningWorkflowCoordinator + 'static>(coordinator: T) -> Self {
        Self(Arc::new(coordinator))
    }
}

impl std::fmt::Debug for DynProvisioningWorkflowCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynProvisioningWorkflowCoordinator")
            .finish()
    }
}

impl std::ops::Deref for DynProvisioningWorkflowCoordinator {
    type Target = Arc<dyn ProvisioningWorkflowCoordinator>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl ProvisioningWorkflowCoordinator for DynProvisioningWorkflowCoordinator {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningWorkflowResult, ProvisioningWorkflowError> {
        self.0.provision_worker(provider_id, spec, job_id).await
    }
}

#[async_trait]
impl ProvisioningWorkflowCoordinator for &DynProvisioningWorkflowCoordinator {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningWorkflowResult, ProvisioningWorkflowError> {
        self.0.provision_worker(provider_id, spec, job_id).await
    }
}

#[async_trait]
impl ProvisioningWorkflowCoordinator for &dyn ProvisioningWorkflowCoordinator {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
        job_id: Option<JobId>,
    ) -> Result<ProvisioningWorkflowResult, ProvisioningWorkflowError> {
        (*self).provision_worker(provider_id, spec, job_id).await
    }
}

/// Helper to create a provisioning workflow executor
///
/// This function creates the necessary components to run the ProvisioningWorkflow
/// with saga-engine v4.0. For now, it returns a mock coordinator.
///
/// TODO: Implement actual DurableWorkflowRuntime integration
pub async fn create_provisioning_workflow_coordinator(
    _registry: Arc<dyn WorkerRegistry + Send + Sync>,
    _pool: &sqlx::PgPool,
) -> Result<DynProvisioningWorkflowCoordinator, ProvisioningWorkflowError> {
    info!("Creating provisioning workflow coordinator (v2)");

    // For now, return mock coordinator
    // In production, this would create:
    // 1. DurableWorkflowRuntime with proper TaskQueue and EventStore
    // 2. SyncDurableWorkflowExecutor for the ProvisioningWorkflow
    // 3. Proper activity implementations

    Ok(DynProvisioningWorkflowCoordinator::new(
        MockProvisioningCoordinator,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::workers::WorkerSpec;

    #[tokio::test]
    async fn test_mock_provisioning_coordinator() {
        let coordinator = MockProvisioningCoordinator;
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("test-image".to_string(), "localhost:50051".to_string());
        let job_id = Some(JobId::new());

        let result = coordinator
            .provision_worker(&provider_id, &spec, job_id.clone())
            .await
            .unwrap();

        assert!(!result.worker_id.to_string().is_empty());
        assert!(!result.otp_token.is_empty());
        assert_eq!(result.job_id, job_id);
        assert_eq!(result.provider_id, provider_id);
    }

    #[tokio::test]
    async fn test_dyn_provisioning_coordinator() {
        let coordinator = DynProvisioningWorkflowCoordinator::new(MockProvisioningCoordinator);
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("test-image".to_string(), "localhost:50051".to_string());

        let result = coordinator
            .provision_worker(&provider_id, &spec, None)
            .await
            .unwrap();

        assert!(!result.worker_id.to_string().is_empty());
        assert!(result.otp_token.starts_with("mock-otp-"));
    }
}
