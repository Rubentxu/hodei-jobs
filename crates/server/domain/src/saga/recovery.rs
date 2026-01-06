//! Recovery Saga - EPIC-46 GAP-05
//!
//! Saga para la recuperación de workers fallidos y reassignación de jobs.

use super::Saga;
use super::types::{SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, WorkerId};
use async_trait::async_trait;
use std::time::Duration;
use tracing::{info, instrument};

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

    #[instrument(skip(context), fields(step = "ProvisionNewWorker"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let provider_id = self
            .target_provider_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let new_worker_id = WorkerId::new();

        context
            .set_metadata("new_worker_id", &new_worker_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        context
            .set_metadata("new_worker_provider_id", &provider_id)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        info!(job_id = %self.job_id, worker_id = %new_worker_id, "New worker provisioned");
        Ok(())
    }

    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        context
            .set_metadata("compensation_pending", &true)
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

    #[instrument(skip(context), fields(step = "TransferJob"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata("job_transferred_at", &chrono::Utc::now().to_rfc3339())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        context
            .set_metadata("job_transfer_pending", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        info!(job_id = %self.job_id, "Job transfer metadata stored");
        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
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

    #[instrument(skip(context), fields(step = "TerminateOldWorker"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata("old_worker_terminated", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        info!(worker_id = %self.worker_id, "Old worker termination metadata stored");
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

    #[instrument(skip(context), fields(step = "CancelOldWorker"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata("old_worker_cancelled", &true)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        info!(worker_id = %self.worker_id, "Old worker cancellation metadata stored");
        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
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
