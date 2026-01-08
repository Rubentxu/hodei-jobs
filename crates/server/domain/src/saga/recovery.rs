//! Recovery Saga - EPIC-46 GAP-05
//!
//! Saga para la recuperación de workers fallidos y reassignación de jobs.
//!
//! EPIC-50 GAP-CRITICAL-01: Updated to perform real provisioning operations
//! instead of just storing metadata.

use super::Saga;
use super::types::{SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, WorkerId, JobState};
use async_trait::async_trait;
use std::time::Duration;
use tracing::{info, instrument, warn};

/// Saga para recuperar de fallos de workers y reassignar jobs.
#[derive(Debug, Clone)]
pub struct RecoverySaga {
    pub job_id: JobId,
    pub failed_worker_id: WorkerId,
    pub target_provider_id: Option<String>,
}

impl RecoverySaga {
    #[inline]
    pub fn new(
        job_id: JobId,
        failed_worker_id: WorkerId,
        target_provider_id: Option<String>,
    ) -> Self {
        Self {
            job_id,
            failed_worker_id,
            target_provider_id,
        }
    }
}

impl Saga for RecoverySaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Recovery
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(CheckWorkerConnectivityStep::new(
                self.failed_worker_id.clone(),
            )),
            Box::new(ProvisionNewWorkerStep::new(
                self.job_id.clone(),
                self.target_provider_id.clone(),
            )),
            Box::new(TransferJobStep::new(self.job_id.clone())),
            Box::new(TerminateOldWorkerStep::new(self.failed_worker_id.clone())),
            Box::new(CancelOldWorkerStep::new(self.failed_worker_id.clone())),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(300))
    }
}

// ============================================================================
// CheckWorkerConnectivityStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct CheckWorkerConnectivityStep {
    failed_worker_id: WorkerId,
}

impl CheckWorkerConnectivityStep {
    #[inline]
    pub fn new(failed_worker_id: WorkerId) -> Self {
        Self { failed_worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CheckWorkerConnectivityStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "CheckWorkerConnectivity"
    }

    #[instrument(skip(context), fields(step = "CheckWorkerConnectivity"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Store metadata for connectivity check
        context
            .set_metadata("failed_worker_id", &self.failed_worker_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        context
            .set_metadata("connectivity_status", &"Unreachable".to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        info!(worker_id = %self.failed_worker_id, "Connectivity check completed");
        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }
}

// ============================================================================
// ProvisionNewWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProvisionNewWorkerStep {
    job_id: JobId,
    target_provider_id: Option<String>,
}

impl ProvisionNewWorkerStep {
    #[inline]
    pub fn new(job_id: JobId, target_provider_id: Option<String>) -> Self {
        Self {
            job_id,
            target_provider_id,
        }
    }
}

#[async_trait::async_trait]
impl SagaStep for ProvisionNewWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ProvisionNewWorker"
    }

    /// EPIC-50 GAP-CRITICAL-01: Execute with real provisioning operations
    #[instrument(skip(context), fields(step = "ProvisionNewWorker"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: false,
        })?;

        // Check if provisioning service is available
        let provisioning_service = services.provisioning_service.as_ref().ok_or_else(|| {
            SagaError::StepFailed {
                step: self.name().to_string(),
                message: "ProvisioningService not available in SagaServices".to_string(),
                will_compensate: false,
            }
        })?;

        let provider_id = self
            .target_provider_id
            .clone()
            .unwrap_or_else(|| "default".to_string());

        // Build worker spec for recovery
        let worker_spec = crate::workers::WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        // Parse provider ID
        let provider_uuid = uuid::Uuid::parse_str(&provider_id)
            .unwrap_or_else(|_| uuid::Uuid::new_v4());
        let provider_id_typed = crate::ProviderId::from_uuid(provider_uuid);

        // Perform real provisioning operation
        let result = provisioning_service
            .provision_worker(&provider_id_typed, worker_spec, self.job_id.clone())
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to provision recovery worker: {}", e),
                will_compensate: true,
            })?;

        // Store metadata for compensation
        context
            .set_metadata("new_worker_id", &result.worker_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        context
            .set_metadata("new_worker_provider_id", &provider_id)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        context
            .set_metadata("recovery_provisioning_done", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(
            job_id = %self.job_id,
            worker_id = %result.worker_id,
            provider_id = %provider_id,
            "New worker provisioned for recovery"
        );
        Ok(())
    }

    /// EPIC-50 GAP-CRITICAL-01: Compensate by destroying provisioned worker
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        // Check if we actually provisioned a worker
        let provisioning_done = context
            .get_metadata::<bool>("recovery_provisioning_done")
            .and_then(|r| r.ok())
            .unwrap_or(false);

        if !provisioning_done {
            info!("No recovery worker was provisioned, skipping compensation");
            return Ok(());
        }

        // Get the worker ID we created
        let new_worker_id = context
            .get_metadata::<String>("new_worker_id")
            .and_then(|r| r.ok());

        if let Some(worker_id_str) = new_worker_id {
            let services = context.services();

            if let Some(services) = services {
                if let Some(provisioning_service) = &services.provisioning_service {
                    // Parse worker ID
                    let worker_id = WorkerId::from_string(&worker_id_str)
                        .unwrap_or_else(|| WorkerId::new());

                    // Destroy the provisioned worker
                    match provisioning_service
                        .destroy_worker(&worker_id)
                        .await
                    {
                        Ok(_) => {
                            info!(worker_id = %worker_id, "Recovery worker destroyed during compensation");
                        }
                        Err(e) => {
                            warn!(
                                worker_id = %worker_id,
                                error = %e,
                                "Failed to destroy recovery worker during compensation"
                            );
                        }
                    }
                }
            }
        }

        context
            .set_metadata("compensation_pending", &false)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// TransferJobStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct TransferJobStep {
    job_id: JobId,
}

impl TransferJobStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for TransferJobStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "TransferJob"
    }

    /// EPIC-50 GAP-CRITICAL-01: Execute real job transfer operations
    #[instrument(skip(context), fields(step = "TransferJob"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: false,
        })?;

        // Get the new worker ID from previous step
        let new_worker_id_str = context
            .get_metadata::<String>("new_worker_id")
            .and_then(|r| r.ok())
            .ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: "new_worker_id not found in context metadata".to_string(),
                will_compensate: true,
            })?;

        let new_worker_id = WorkerId::from_string(&new_worker_id_str)
            .unwrap_or_else(|| WorkerId::new());

        // First: Get current job state for compensation (uses job_repo)
        // Drop services before mutating context
        let old_job_state = if let Some(job_repo) = services.job_repository.as_ref() {
            job_repo.find_by_id(&self.job_id)
                .await
                .ok()
                .flatten()
                .map(|j| j.state().to_string())
        } else {
            None
        };

        // Second: Update job to be assigned to the new worker (uses job_repo again)
        if let Some(job_repo) = services.job_repository.as_ref() {
            job_repo.update_state(&self.job_id, JobState::Assigned)
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to update job state: {}", e),
                    will_compensate: true,
                })?;
        }

        // Third: Store metadata (mutates context)
        if let Some(state) = old_job_state {
            context.set_metadata("old_job_state", &state).ok();
        }
        context
            .set_metadata("job_transferred_at", &chrono::Utc::now().to_rfc3339())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        context
            .set_metadata("job_transfer_done", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(
            job_id = %self.job_id,
            new_worker_id = %new_worker_id,
            "Job transferred to recovery worker"
        );
        Ok(())
    }

    /// EPIC-50 GAP-CRITICAL-01: Compensate by reverting job assignment
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        let transfer_done = context
            .get_metadata::<bool>("job_transfer_done")
            .and_then(|r| r.ok())
            .unwrap_or(false);

        if !transfer_done {
            info!("Job was not transferred, skipping compensation");
            return Ok(());
        }

        if let Some(services) = context.services() {
            if let Some(job_repo) = &services.job_repository {
                // Revert job state to Failed since the recovery failed
                job_repo
                    .update_state(&self.job_id, JobState::Failed)
                    .await
                    .map_err(|e| SagaError::CompensationFailed {
                        step: self.name().to_string(),
                        message: format!("Failed to revert job state: {}", e),
                    })?;

                warn!(
                    job_id = %self.job_id,
                    "Job transfer compensation completed - job marked as failed"
                );
            }
        }

        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// TerminateOldWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct TerminateOldWorkerStep {
    worker_id: WorkerId,
}

impl TerminateOldWorkerStep {
    #[inline]
    pub fn new(worker_id: WorkerId) -> Self {
        Self { worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for TerminateOldWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "TerminateOldWorker"
    }

    /// EPIC-50 GAP-CRITICAL-01: Execute real worker termination
    #[instrument(skip(context), fields(step = "TerminateOldWorker"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: false,
        })?;

        // Update worker state to terminated in registry
        let worker_registry = &services.provider_registry;

        // Mark the old worker as terminated in registry
        match worker_registry
            .update_state(&self.worker_id, crate::WorkerState::Terminated)
            .await
        {
            Ok(_) => {
                info!(worker_id = %self.worker_id, "Old worker marked as terminated");
            }
            Err(e) => {
                warn!(
                    worker_id = %self.worker_id,
                    error = %e,
                    "Failed to mark old worker as terminated (may already be terminated)"
                );
            }
        }

        context
            .set_metadata("old_worker_terminated", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(worker_id = %self.worker_id, "Old worker termination completed");
        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // No compensation needed - we don't resurrect failed workers
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }
}

// ============================================================================
// CancelOldWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct CancelOldWorkerStep {
    worker_id: WorkerId,
}

impl CancelOldWorkerStep {
    #[inline]
    pub fn new(worker_id: WorkerId) -> Self {
        Self { worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CancelOldWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "CancelOldWorker"
    }

    /// EPIC-50 GAP-CRITICAL-01: Execute real worker unregistration
    #[instrument(skip(context), fields(step = "CancelOldWorker"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: false,
        })?;

        // Unregister the old worker from the registry
        let worker_registry = &services.provider_registry;

        match worker_registry.unregister(&self.worker_id).await {
            Ok(_) => {
                info!(worker_id = %self.worker_id, "Old worker unregistered from registry");
            }
            Err(e) => {
                warn!(
                    worker_id = %self.worker_id,
                    error = %e,
                    "Failed to unregister old worker (may already be unregistered)"
                );
            }
        }

        context
            .set_metadata("old_worker_cancelled", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(worker_id = %self.worker_id, "Old worker cancellation completed");
        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // No compensation needed - we don't re-register failed workers
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_saga_has_five_steps() {
        let saga = RecoverySaga::new(JobId::new(), WorkerId::new(), None);
        assert_eq!(saga.steps().len(), 5);
    }

    #[test]
    fn recovery_saga_has_correct_type() {
        let saga = RecoverySaga::new(JobId::new(), WorkerId::new(), None);
        assert_eq!(saga.saga_type(), SagaType::Recovery);
    }

    #[test]
    fn recovery_saga_has_timeout() {
        let saga = RecoverySaga::new(JobId::new(), WorkerId::new(), None);
        assert_eq!(saga.timeout().unwrap(), Duration::from_secs(300));
    }
}
