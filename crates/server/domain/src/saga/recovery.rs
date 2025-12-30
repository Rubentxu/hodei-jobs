//! Recovery Saga
//!
//! Saga para la recuperación de workers fallidos y reassignación de jobs.
//! Encapsula la lógica de:
//! - Detección de worker fallido
//! - Verificación de si el worker realmente se cayó
//! - Transferencia del job a nuevo worker si es necesario
//! - Terminación del worker viejo

use crate::saga::{Saga, SagaContext, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::JobId;

use chrono::Utc;
use std::time::Duration;

// ============================================================================
// RecoverySaga
// ============================================================================

/// Saga para recuperar de fallos de workers y reassignar jobs.
#[derive(Debug, Clone)]
pub struct RecoverySaga {
    pub job_id: JobId,
    pub failed_worker_id: JobId,
}

impl RecoverySaga {
    #[inline]
    pub fn new(job_id: JobId, failed_worker_id: JobId) -> Self {
        Self {
            job_id,
            failed_worker_id,
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
            Box::new(ProvisionNewWorkerStep::new(self.job_id.clone())),
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
    failed_worker_id: JobId,
}

impl CheckWorkerConnectivityStep {
    #[inline]
    pub fn new(failed_worker_id: JobId) -> Self {
        Self { failed_worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CheckWorkerConnectivityStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "CheckWorkerConnectivity"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata(
                "recovery_failed_worker_id",
                &self.failed_worker_id.to_string(),
            )
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        true
    }

    #[inline]
    fn has_compensation(&self) -> bool {
        false
    }
}

// ============================================================================
// ProvisionNewWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProvisionNewWorkerStep {
    job_id: JobId,
}

impl ProvisionNewWorkerStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ProvisionNewWorkerStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "ProvisionNewWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata(
                "recovery_new_worker_id",
                &format!("new-{}", uuid::Uuid::new_v4()),
            )
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
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

    #[inline]
    fn name(&self) -> &'static str {
        "TransferJob"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata("job_transferred_at", &Utc::now().to_rfc3339())
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// TerminateOldWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct TerminateOldWorkerStep {
    worker_id: JobId,
}

impl TerminateOldWorkerStep {
    #[inline]
    pub fn new(worker_id: JobId) -> Self {
        Self { worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for TerminateOldWorkerStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "TerminateOldWorker"
    }

    async fn execute(&self, _context: &mut SagaContext) -> SagaResult<Self::Output> {
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        true
    }
}

// ============================================================================
// CancelOldWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct CancelOldWorkerStep {
    worker_id: JobId,
}

impl CancelOldWorkerStep {
    #[inline]
    pub fn new(worker_id: JobId) -> Self {
        Self { worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CancelOldWorkerStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "CancelOldWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let is_connected = context
            .get_metadata::<bool>("worker_reconnected")
            .and_then(|r| r.ok())
            .unwrap_or(false);

        if !is_connected {
            return Ok(());
        }

        context
            .set_metadata("old_worker_cancelled", &true)
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        true
    }

    #[inline]
    fn has_compensation(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_saga_should_have_five_steps() {
        let saga = RecoverySaga::new(JobId::new(), JobId::new());
        let steps = saga.steps();
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0].name(), "CheckWorkerConnectivity");
        assert_eq!(steps[1].name(), "ProvisionNewWorker");
        assert_eq!(steps[2].name(), "TransferJob");
        assert_eq!(steps[3].name(), "TerminateOldWorker");
        assert_eq!(steps[4].name(), "CancelOldWorker");
    }

    #[test]
    fn recovery_saga_has_correct_type() {
        let saga = RecoverySaga::new(JobId::new(), JobId::new());
        assert_eq!(saga.saga_type(), SagaType::Recovery);
    }

    #[test]
    fn recovery_saga_has_timeout() {
        let saga = RecoverySaga::new(JobId::new(), JobId::new());
        assert!(saga.timeout().is_some());
        assert_eq!(saga.timeout().unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn check_worker_connectivity_step_has_no_compensation() {
        let step = CheckWorkerConnectivityStep::new(JobId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn terminate_old_worker_step_is_idempotent() {
        let step = TerminateOldWorkerStep::new(JobId::new());
        assert!(step.is_idempotent());
    }

    #[test]
    fn cancel_old_worker_step_has_no_compensation() {
        let step = CancelOldWorkerStep::new(JobId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }
}
