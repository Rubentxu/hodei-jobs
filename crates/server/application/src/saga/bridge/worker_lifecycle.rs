//!
//! # Worker Lifecycle Management Activity
//!

use async_trait::async_trait;
use saga_engine_core::workflow::Activity;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use thiserror::Error;

use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::{WorkerProvisioning, WorkerRegistry, WorkerSpec};
use hodei_shared::states::WorkerState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLifecycleInput {
    pub spec: WorkerSpec,
    pub provider_id: ProviderId,
    pub job_id: JobId,
    pub saga_id: Option<String>,
}

impl WorkerLifecycleInput {
    pub fn new(
        spec: WorkerSpec,
        provider_id: ProviderId,
        job_id: JobId,
        saga_id: Option<String>,
    ) -> Self {
        Self {
            spec,
            provider_id,
            job_id,
            saga_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLifecycleOutput {
    pub worker_id: WorkerId,
    pub provider_worker_id: String,
    pub state: WorkerState,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl WorkerLifecycleOutput {
    pub fn success(worker_id: WorkerId, provider_worker_id: String, state: WorkerState) -> Self {
        Self {
            worker_id,
            provider_worker_id,
            state,
            created_at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Error)]
pub enum WorkerLifecycleError {
    #[error("Provider {provider_id} is not available")]
    ProviderNotAvailable { provider_id: ProviderId },
    #[error("Failed to provision worker")]
    ProvisioningFailed,
    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },
    #[error("Failed to update worker state")]
    StateUpdateFailed,
    #[error("Worker termination failed")]
    TerminationFailed,
}

#[derive(Clone)]
pub struct ProvisionWorkerActivity {
    provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
}

impl Debug for ProvisionWorkerActivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionWorkerActivity").finish()
    }
}

impl ProvisionWorkerActivity {
    pub fn new(provisioning: Arc<dyn WorkerProvisioning + Send + Sync>) -> Self {
        Self { provisioning }
    }
}

#[async_trait]
impl Activity for ProvisionWorkerActivity {
    const TYPE_ID: &'static str = "worker-provision";

    type Input = WorkerLifecycleInput;
    type Output = WorkerLifecycleOutput;
    type Error = WorkerLifecycleError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let provider_id = input.provider_id.clone();
        let available = self
            .provisioning
            .is_provider_available(&provider_id)
            .await
            .map_err(|_| WorkerLifecycleError::ProviderNotAvailable {
                provider_id: provider_id.clone(),
            })?;

        if !available {
            return Err(WorkerLifecycleError::ProviderNotAvailable {
                provider_id: provider_id.clone(),
            });
        }

        let spec = input.spec.clone();
        let job_id = input.job_id.clone();
        let result = self
            .provisioning
            .provision_worker(&provider_id, spec, job_id)
            .await
            .map_err(|_| WorkerLifecycleError::ProvisioningFailed)?;

        let worker_id = result.worker_id.clone();
        Ok(WorkerLifecycleOutput::success(
            worker_id.clone(),
            format!("provider-{}", worker_id),
            WorkerState::Ready,
        ))
    }
}

#[derive(Clone)]
pub struct RegisterWorkerActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for RegisterWorkerActivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisterWorkerActivity").finish()
    }
}

impl RegisterWorkerActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for RegisterWorkerActivity {
    const TYPE_ID: &'static str = "worker-register";

    type Input = RegisterWorkerInput;
    type Output = RegisterWorkerOutput;
    type Error = WorkerLifecycleError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.registry
            .find_by_id(&input.worker_id)
            .await
            .map_err(|_| WorkerLifecycleError::StateUpdateFailed)?
            .ok_or_else(|| WorkerLifecycleError::WorkerNotFound {
                worker_id: input.worker_id.clone(),
            })?;

        self.registry
            .update_state(&input.worker_id, WorkerState::Creating)
            .await
            .map_err(|_| WorkerLifecycleError::StateUpdateFailed)?;

        Ok(RegisterWorkerOutput {
            worker_id: input.worker_id,
            state: WorkerState::Creating,
            registered_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerInput {
    pub worker_id: WorkerId,
    pub otp: String,
    pub saga_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerOutput {
    pub worker_id: WorkerId,
    pub state: WorkerState,
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct SetWorkerReadyActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for SetWorkerReadyActivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetWorkerReadyActivity").finish()
    }
}

impl SetWorkerReadyActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for SetWorkerReadyActivity {
    const TYPE_ID: &'static str = "worker-set-ready";

    type Input = WorkerId;
    type Output = WorkerState;
    type Error = WorkerLifecycleError;

    async fn execute(&self, worker_id: Self::Input) -> Result<Self::Output, Self::Error> {
        self.registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .map_err(|_| WorkerLifecycleError::StateUpdateFailed)?;
        Ok(WorkerState::Ready)
    }
}

#[derive(Clone)]
pub struct SetWorkerBusyActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for SetWorkerBusyActivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetWorkerBusyActivity").finish()
    }
}

impl SetWorkerBusyActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Activity for SetWorkerBusyActivity {
    const TYPE_ID: &'static str = "worker-set-busy";

    type Input = BusyWorkerInput;
    type Output = WorkerState;
    type Error = WorkerLifecycleError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.registry
            .update_state(&input.worker_id, WorkerState::Busy)
            .await
            .map_err(|_| WorkerLifecycleError::StateUpdateFailed)?;
        Ok(WorkerState::Busy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusyWorkerInput {
    pub worker_id: WorkerId,
    pub job_id: JobId,
}

#[derive(Clone)]
pub struct TerminateWorkerActivity {
    provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for TerminateWorkerActivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminateWorkerActivity").finish()
    }
}

impl TerminateWorkerActivity {
    pub fn new(
        provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self {
            provisioning,
            registry,
        }
    }
}

#[async_trait]
impl Activity for TerminateWorkerActivity {
    const TYPE_ID: &'static str = "worker-terminate";

    type Input = TerminateWorkerInput;
    type Output = TerminateWorkerOutput;
    type Error = WorkerLifecycleError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.registry
            .find_by_id(&input.worker_id)
            .await
            .map_err(|_| WorkerLifecycleError::TerminationFailed)?
            .ok_or_else(|| WorkerLifecycleError::WorkerNotFound {
                worker_id: input.worker_id.clone(),
            })?;

        self.registry
            .update_state(&input.worker_id, WorkerState::Terminated)
            .await
            .map_err(|_| WorkerLifecycleError::TerminationFailed)?;

        if input.destroy_infrastructure {
            self.provisioning
                .destroy_worker(&input.worker_id)
                .await
                .map_err(|_| WorkerLifecycleError::TerminationFailed)?;
        }

        Ok(TerminateWorkerOutput {
            worker_id: input.worker_id,
            terminated: true,
            infrastructure_destroyed: input.destroy_infrastructure,
            terminated_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateWorkerInput {
    pub worker_id: WorkerId,
    pub reason: String,
    pub destroy_infrastructure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateWorkerOutput {
    pub worker_id: WorkerId,
    pub terminated: bool,
    pub infrastructure_destroyed: bool,
    pub terminated_at: chrono::DateTime<chrono::Utc>,
}

pub struct WorkerStateTransitions;

impl WorkerStateTransitions {
    pub fn is_valid(current: &WorkerState, desired: &WorkerState) -> bool {
        match (current, desired) {
            (WorkerState::Creating, WorkerState::Ready) => true,
            (WorkerState::Ready, WorkerState::Busy) => true,
            (WorkerState::Busy, WorkerState::Ready) => true,
            (WorkerState::Ready, WorkerState::Terminated) => true,
            (WorkerState::Busy, WorkerState::Terminated) => true,
            (WorkerState::Creating, WorkerState::Terminated) => true,
            (s, d) if s == d => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::shared_kernel::ProviderId;
    use hodei_server_domain::workers::WorkerSpec;

    fn make_test_worker_spec() -> WorkerSpec {
        WorkerSpec::new(
            "test-image:latest".to_string(),
            "localhost:50051".to_string(),
        )
    }

    fn make_test_input() -> WorkerLifecycleInput {
        WorkerLifecycleInput::new(
            make_test_worker_spec(),
            ProviderId::new(),
            JobId::new(),
            Some("test-saga".to_string()),
        )
    }

    #[test]
    fn test_worker_lifecycle_input() {
        let input = make_test_input();
        assert!(input.saga_id.is_some());
        assert_eq!(input.saga_id.unwrap(), "test-saga");
    }

    #[test]
    fn test_worker_lifecycle_output() {
        let worker_id = WorkerId::new();
        let output = WorkerLifecycleOutput::success(
            worker_id.clone(),
            "provider-worker-123".to_string(),
            WorkerState::Ready,
        );
        assert_eq!(output.worker_id, worker_id);
        assert_eq!(output.state, WorkerState::Ready);
    }

    #[test]
    fn test_valid_state_transitions() {
        assert!(WorkerStateTransitions::is_valid(
            &WorkerState::Creating,
            &WorkerState::Ready
        ));
        assert!(WorkerStateTransitions::is_valid(
            &WorkerState::Ready,
            &WorkerState::Busy
        ));
        assert!(WorkerStateTransitions::is_valid(
            &WorkerState::Busy,
            &WorkerState::Ready
        ));
        assert!(WorkerStateTransitions::is_valid(
            &WorkerState::Ready,
            &WorkerState::Terminated
        ));
    }

    #[test]
    fn test_invalid_state_transitions() {
        assert!(!WorkerStateTransitions::is_valid(
            &WorkerState::Ready,
            &WorkerState::Creating
        ));
        assert!(!WorkerStateTransitions::is_valid(
            &WorkerState::Busy,
            &WorkerState::Creating
        ));
        assert!(!WorkerStateTransitions::is_valid(
            &WorkerState::Terminated,
            &WorkerState::Ready
        ));
    }
}
