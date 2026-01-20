//! Worker Lifecycle Facade
//!
//! EPIC-33: Thin facade that coordinates specialized worker lifecycle services.

use crate::saga::provisioning_saga::DynProvisioningSagaCoordinator;
use crate::saga::recovery_saga::DynRecoverySagaCoordinator;
use crate::workers::pulse::WorkerPulseService;
use crate::workers::termination::WorkerTerminationService;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::shared_kernel::{JobId, ProviderId, Result, WorkerId};
use hodei_server_domain::workers::{WorkerRegistry, WorkerSpec};
use std::sync::Arc;
use tracing::info;

pub use super::lifecycle::WorkerLifecycleConfig;

pub struct WorkerLifecycleFacade {
    pulse_service: Arc<WorkerPulseService>,
    termination_service: Arc<WorkerTerminationService>,
    provisioning_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
    recovery_coordinator: Option<Arc<DynRecoverySagaCoordinator>>,
    event_bus: Arc<dyn EventBus>,
    registry: Arc<dyn WorkerRegistry>,
    config: WorkerLifecycleConfig,
}

impl WorkerLifecycleFacade {
    pub fn new(
        pulse_service: Arc<WorkerPulseService>,
        termination_service: Arc<WorkerTerminationService>,
        event_bus: Arc<dyn EventBus>,
        registry: Arc<dyn WorkerRegistry>,
    ) -> Self {
        Self {
            pulse_service,
            termination_service,
            provisioning_coordinator: None,
            recovery_coordinator: None,
            event_bus,
            registry,
            config: WorkerLifecycleConfig::default(),
        }
    }

    pub fn with_config(mut self, config: WorkerLifecycleConfig) -> Self {
        self.config = config;
        self
    }

    pub fn set_provisioning_coordinator(
        &mut self,
        coordinator: Arc<DynProvisioningSagaCoordinator>,
    ) {
        self.provisioning_coordinator = Some(coordinator);
    }

    pub fn set_recovery_coordinator(&mut self, coordinator: Arc<DynRecoverySagaCoordinator>) {
        self.recovery_coordinator = Some(coordinator);
    }

    pub async fn record_heartbeat(&self, worker_id: &WorkerId) -> Result<()> {
        self.pulse_service.record_heartbeat(worker_id).await?;
        Ok(())
    }

    pub async fn detect_stale_workers(&self) -> Result<Vec<WorkerId>> {
        let result = self.pulse_service.detect_stale_workers(None).await?;
        Ok(result.stale_workers)
    }

    pub async fn terminate_ephemeral_worker(&self, worker_id: &WorkerId) -> Result<()> {
        self.termination_service
            .terminate_ephemeral_worker(worker_id)
            .await?;
        Ok(())
    }

    pub async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
    ) -> Result<WorkerId> {
        let current_count = self.registry.count().await?;
        if current_count >= self.config.max_workers {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::ProviderOverloaded {
                    provider_id: provider_id.clone(),
                },
            );
        }

        if let Some(ref coordinator) = self.provisioning_coordinator {
            info!(%provider_id, "Provisioning worker via saga");
            match coordinator
                .execute_provisioning_saga(provider_id, &spec, None)
                .await
            {
                Ok(result) => {
                    info!(%provider_id, worker_id = %result.worker_id, "Worker provisioned via saga");
                    return Ok(result.worker_id);
                }
                Err(e) => {
                    return Err(
                        hodei_server_domain::shared_kernel::DomainError::WorkerProvisioningFailed {
                            message: e.to_string(),
                        },
                    );
                }
            }
        }

        Err(
            hodei_server_domain::shared_kernel::DomainError::WorkerProvisioningFailed {
                message: "No saga coordinator configured".to_string(),
            },
        )
    }

    pub async fn recover_worker(&self, job_id: &JobId, failed_worker_id: &WorkerId) -> Result<()> {
        if let Some(ref coordinator) = self.recovery_coordinator {
            info!(%job_id, %failed_worker_id, "Recovering worker via saga");
            let reason = format!("Worker {} failed", failed_worker_id);
            match coordinator
                .as_ref()
                .execute_recovery_saga(job_id.clone(), failed_worker_id.clone(), reason)
                .await
            {
                Ok(_) => {
                    info!(%job_id, "Worker recovered via saga");
                    return Ok(());
                }
                Err(e) => {
                    return Err(
                        hodei_server_domain::shared_kernel::DomainError::WorkerRecoveryFailed {
                            message: e.to_string(),
                        },
                    );
                }
            }
        }

        Err(
            hodei_server_domain::shared_kernel::DomainError::WorkerRecoveryFailed {
                message: "No recovery saga coordinator configured".to_string(),
            },
        )
    }

    pub fn is_saga_provisioning_enabled(&self) -> bool {
        self.provisioning_coordinator.is_some()
    }

    pub fn is_saga_recovery_enabled(&self) -> bool {
        self.recovery_coordinator.is_some()
    }

    pub async fn process_heartbeat(&self, worker_id: &WorkerId) -> Result<()> {
        self.record_heartbeat(worker_id).await
    }

    pub async fn stats(&self) -> Result<hodei_server_domain::workers::WorkerRegistryStats> {
        self.registry.stats().await
    }
}

pub use super::lifecycle::WorkerLifecycleManager;
pub use hodei_server_domain::workers::WorkerRegistryStats;
