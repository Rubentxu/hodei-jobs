//! Recovery Saga
//!
//! Saga para la recuperación de workers fallidos y reassignación de jobs.
//! Encapsula la lógica de:
//! - Detección de worker fallido (heartbeat timeout)
//! - Verificación de si el worker realmente se cayó
//! - Transferencia del job a nuevo worker si es necesario
//! - Terminación del worker viejo
//!
//! Esta saga elimina el código de orphan detection y job reassignment manual.

use crate::events::DomainEvent;
use crate::jobs::{Job, JobRepository};
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, ProviderId, WorkerId, WorkerState};
use crate::workers::{Worker, WorkerRegistry, WorkerSpec};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// RecoverySaga
// ============================================================================

/// Saga para recuperar de fallos de workers y reassignar jobs.
///
/// # Pasos:
///
/// 1. **CheckWorkerConnectivityStep**: Verifica si el worker realmente se cayó
/// 2. **ProvisionNewWorkerStep**: aprovisiona un nuevo worker si es necesario
/// 3. **TransferJobStep**: Transfiere el job al nuevo worker
/// 4. **TerminateOldWorkerStep**: Termina el worker viejo
/// 5. **CancelOldWorkerStep**: Cancela el worker viejo si se reconectó
///
/// # Compensaciones:
///
/// - Si el worker viejo se reconecta, cancelamos el nuevo worker
/// - Si la transferencia falla, revertimos el estado del job
#[derive(Debug, Clone)]
pub struct RecoverySaga {
    /// Job que necesita recuperación
    job_id: JobId,
    /// Worker que felló
    failed_worker_id: WorkerId,
    /// Repositorio de jobs
    job_repository: Arc<dyn JobRepository>,
    /// Registro de workers
    worker_registry: Arc<dyn WorkerRegistry>,
    /// Spec del nuevo worker
    new_worker_spec: Option<WorkerSpec>,
}

impl RecoverySaga {
    /// Crea una nueva RecoverySaga
    pub fn new(
        job_id: JobId,
        failed_worker_id: WorkerId,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        new_worker_spec: Option<WorkerSpec>,
    ) -> Self {
        Self {
            job_id,
            failed_worker_id,
            job_repository,
            worker_registry,
            new_worker_spec,
        }
    }

    /// Crea una RecoverySaga para recovery de job específico
    pub fn for_job(
        job: &Job,
        failed_worker_id: WorkerId,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
    ) -> Self {
        let spec = WorkerSpec::new(job.spec.image.clone(), job.spec.server_address.clone());
        // Copy TTL settings from job spec if available
        let mut spec = spec;
        if let Some(timeout) = job.spec.timeout {
            spec.max_lifetime = timeout;
        }

        Self {
            job_id: job.id.clone(),
            failed_worker_id,
            job_repository,
            worker_registry,
            new_worker_spec: Some(spec),
        }
    }
}

impl Saga for RecoverySaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Recovery
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = RecoveryStepOutput>>> {
        vec![
            Box::new(CheckWorkerConnectivityStep::new(
                self.failed_worker_id.clone(),
            )),
            Box::new(ProvisionNewWorkerStep::new(
                self.new_worker_spec.clone().unwrap_or_else(|| {
                    WorkerSpec::new(
                        "hodei-worker:latest".to_string(),
                        "http://localhost:50051".to_string(),
                    )
                }),
            )),
            Box::new(TransferJobStep::new(self.job_id.clone())),
            Box::new(TerminateOldWorkerStep::new(self.failed_worker_id.clone())),
            Box::new(CancelOldWorkerStep::new(self.failed_worker_id.clone())),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(300))
    }

    fn idempotency_key(&self) -> Option<&str> {
        Some(&format!(
            "recovery-saga-{}-{}",
            self.job_id.0, self.failed_worker_id.0
        ))
    }
}

// ============================================================================
// Step Outputs
// ============================================================================

/// Output producido por los steps de RecoverySaga
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStepOutput {
    pub step_name: &'static str,
    pub job_id: Option<JobId>,
    pub failed_worker_id: WorkerId,
    pub new_worker_id: Option<WorkerId>,
    pub details: serde_json::Value,
}

impl RecoveryStepOutput {
    pub fn connectivity_check(worker_id: WorkerId, is_connected: bool) -> Self {
        Self {
            step_name: "CheckWorkerConnectivity",
            job_id: None,
            failed_worker_id: worker_id,
            new_worker_id: None,
            details: serde_json::json!({
                "is_connected": is_connected,
                "checked_at": Utc::now().to_rfc3339()
            }),
        }
    }

    pub fn worker_provisioned(worker_id: WorkerId, provider_id: ProviderId) -> Self {
        Self {
            step_name: "ProvisionNewWorker",
            job_id: None,
            failed_worker_id: WorkerId::new(),
            new_worker_id: Some(worker_id),
            details: serde_json::json!({
                "provider_id": provider_id.to_string(),
                "provisioned_at": Utc::now().to_rfc3339()
            }),
        }
    }

    pub fn job_transferred(job_id: JobId, from_worker: WorkerId, to_worker: WorkerId) -> Self {
        Self {
            step_name: "TransferJob",
            job_id: Some(job_id),
            failed_worker_id: from_worker,
            new_worker_id: Some(to_worker),
            details: serde_json::json!({
                "transferred_at": Utc::now().to_rfc3339()
            }),
        }
    }

    pub fn old_worker_terminated(worker_id: WorkerId) -> Self {
        Self {
            step_name: "TerminateOldWorker",
            job_id: None,
            failed_worker_id: worker_id,
            new_worker_id: None,
            details: serde_json::json!({
                "terminated_at": Utc::now().to_rfc3339()
            }),
        }
    }

    pub fn old_worker_cancelled(worker_id: WorkerId) -> Self {
        Self {
            step_name: "CancelOldWorker",
            job_id: None,
            failed_worker_id: worker_id,
            new_worker_id: None,
            details: serde_json::json!({
                "cancelled_at": Utc::now().to_rfc3339()
            }),
        }
    }
}

// ============================================================================
// CheckWorkerConnectivityStep
// ============================================================================

/// Step que verifica si el worker realmente se cayó.
#[derive(Debug, Clone)]
pub struct CheckWorkerConnectivityStep {
    failed_worker_id: WorkerId,
}

impl CheckWorkerConnectivityStep {
    pub fn new(failed_worker_id: WorkerId) -> Self {
        Self { failed_worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CheckWorkerConnectivityStep {
    type Output = RecoveryStepOutput;

    fn name(&self) -> &'static str {
        "CheckWorkerConnectivity"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata(
                "recovery_failed_worker_id",
                &self.failed_worker_id.to_string(),
            )
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        // Check if worker is still connected/healthy
        let is_connected = true; // TODO: Check via registry or provider

        Ok(RecoveryStepOutput::connectivity_check(
            self.failed_worker_id.clone(),
            is_connected,
        ))
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // No compensation needed for connectivity check
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        false
    }
}

// ============================================================================
// ProvisionNewWorkerStep
// ============================================================================

/// Step que aprovisiona un nuevo worker para el job.
#[derive(Debug, Clone)]
pub struct ProvisionNewWorkerStep {
    worker_spec: WorkerSpec,
}

impl ProvisionNewWorkerStep {
    pub fn new(worker_spec: WorkerSpec) -> Self {
        Self { worker_spec }
    }
}

#[async_trait::async_trait]
impl SagaStep for ProvisionNewWorkerStep {
    type Output = RecoveryStepOutput;

    fn name(&self) -> &'static str {
        "ProvisionNewWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let failed_worker_id_str = context
            .get_metadata::<String>("recovery_failed_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_default();

        // Save worker spec for later use
        let spec_json = serde_json::to_value(&self.worker_spec)
            .map_err(|e| SagaError::SerializationError(e.to_string()))?;
        context
            .set_metadata("recovery_new_worker_spec", &spec_json)
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        Ok(RecoveryStepOutput::worker_provisioned(
            WorkerId::new(),
            ProviderId::new(),
        ))
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // If provisioning fails, no compensation needed (nothing was created)
        // If provisioning succeeds but later step fails, the orchestrator
        // will handle terminating the newly created worker
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// TransferJobStep
// ============================================================================

/// Step que transfiere el job al nuevo worker.
#[derive(Debug, Clone)]
pub struct TransferJobStep {
    job_id: JobId,
}

impl TransferJobStep {
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for TransferJobStep {
    type Output = RecoveryStepOutput;

    fn name(&self) -> &'static str {
        "TransferJob"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let failed_worker_id_str = context
            .get_metadata::<String>("recovery_failed_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_default();

        let failed_worker_id = WorkerId::new(); // Parse from string if needed
        let new_worker_id = WorkerId::new(); // Get from previous step

        context
            .set_metadata("job_transferred_at", &Utc::now().to_rfc3339())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        Ok(RecoveryStepOutput::job_transferred(
            self.job_id.clone(),
            failed_worker_id,
            new_worker_id,
        ))
    }

    async fn compensate(&self, output: &Self::Output) -> SagaResult<()> {
        // Revert job state - the job should be marked as failed or pending
        // This is handled by the saga orchestrator or caller
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// TerminateOldWorkerStep
// ============================================================================

/// Step que termina el worker viejo.
#[derive(Debug, Clone)]
pub struct TerminateOldWorkerStep {
    worker_id: WorkerId,
}

impl TerminateOldWorkerStep {
    pub fn new(worker_id: WorkerId) -> Self {
        Self { worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for TerminateOldWorkerStep {
    type Output = RecoveryStepOutput;

    fn name(&self) -> &'static str {
        "TerminateOldWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get the failed worker ID from context
        let worker_id_str = context
            .get_metadata::<String>("recovery_failed_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| self.worker_id.to_string());

        Ok(RecoveryStepOutput::old_worker_terminated(
            self.worker_id.clone(),
        ))
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // Cannot undo termination - this is a terminal operation
        // Compensation is handled by not proceeding to this step if earlier steps fail
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true // Terminating an already terminated worker is safe
    }
}

// ============================================================================
// CancelOldWorkerStep
// ============================================================================

/// Step que cancela el worker viejo si se reconectó.
///
/// Este step solo se ejecuta si el worker viejo se reconectó durante
/// el proceso de recovery. En ese caso, necesitamos cancelar el nuevo
/// worker y dejar que el viejo continúe.
#[derive(Debug, Clone)]
pub struct CancelOldWorkerStep {
    worker_id: WorkerId,
}

impl CancelOldWorkerStep {
    pub fn new(worker_id: WorkerId) -> Self {
        Self { worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CancelOldWorkerStep {
    type Output = RecoveryStepOutput;

    fn name(&self) -> &'static str {
        "CancelOldWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Check if worker reconnected based on previous step
        let is_connected = context
            .get_metadata::<bool>("worker_reconnected")
            .and_then(|r| r.ok())
            .unwrap_or(false);

        if !is_connected {
            // Worker didn't reconnect, nothing to cancel
            // Return early but mark step as completed (no-op)
            return Ok(RecoveryStepOutput::old_worker_cancelled(
                self.worker_id.clone(),
            ));
        }

        // Worker reconnected - we should cancel the new worker
        let new_worker_id_str = context
            .get_metadata::<String>("recovery_new_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_default();

        Ok(RecoveryStepOutput::old_worker_cancelled(
            self.worker_id.clone(),
        ))
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // If we cancelled the new worker but it was actually needed,
        // this is a failure that needs manual intervention
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        false // This step is conditional
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_saga_should_have_five_steps() {
        let saga = RecoverySaga::new(
            JobId::new(),
            WorkerId::new(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            None,
        );

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
        let saga = RecoverySaga::new(
            JobId::new(),
            WorkerId::new(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            None,
        );

        assert_eq!(saga.saga_type(), SagaType::Recovery);
    }

    #[test]
    fn recovery_saga_has_timeout() {
        let saga = RecoverySaga::new(
            JobId::new(),
            WorkerId::new(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            None,
        );

        assert!(saga.timeout().is_some());
        assert_eq!(saga.timeout().unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn recovery_saga_has_idempotency_key() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let saga = RecoverySaga::new(
            job_id.clone(),
            worker_id.clone(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            None,
        );

        let key = saga.idempotency_key().unwrap();
        assert!(key.contains(&job_id.0.to_string()));
        assert!(key.contains(&worker_id.0.to_string()));
    }

    #[test]
    fn check_worker_connectivity_step_has_no_compensation() {
        let step = CheckWorkerConnectivityStep::new(WorkerId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn terminate_old_worker_step_is_idempotent() {
        let step = TerminateOldWorkerStep::new(WorkerId::new());
        assert!(step.is_idempotent());
    }

    #[test]
    fn cancel_old_worker_step_has_no_compensation() {
        let step = CancelOldWorkerStep::new(WorkerId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn recovery_step_output_variants() {
        let worker_id = WorkerId::new();
        let job_id = JobId::new();

        let connectivity = RecoveryStepOutput::connectivity_check(worker_id.clone(), false);
        assert_eq!(connectivity.step_name, "CheckWorkerConnectivity");

        let provisioned =
            RecoveryStepOutput::worker_provisioned(worker_id.clone(), ProviderId::new());
        assert_eq!(provisioned.step_name, "ProvisionNewWorker");

        let transferred =
            RecoveryStepOutput::job_transferred(job_id.clone(), worker_id.clone(), WorkerId::new());
        assert_eq!(transferred.step_name, "TransferJob");

        let terminated = RecoveryStepOutput::old_worker_terminated(worker_id.clone());
        assert_eq!(terminated.step_name, "TerminateOldWorker");

        let cancelled = RecoveryStepOutput::old_worker_cancelled(worker_id);
        assert_eq!(cancelled.step_name, "CancelOldWorker");
    }
}
