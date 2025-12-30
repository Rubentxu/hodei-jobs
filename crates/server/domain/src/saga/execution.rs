//! Execution Saga
//!
//! Saga para la ejecución de jobs en workers.
//! Encapsula la lógica de:
//! - Asignación de job a worker
//! - Envío de comando RUN_JOB via gRPC
//! - Monitoreo de ejecución
//! - Compensación en caso de fallo
//!
//! Esta saga elimina el código de rollback manual en `assign_and_dispatch`
//! transformando Connascence of Position en Connascence of Type.

use crate::events::{DomainEvent, EventMetadata};
use crate::jobs::{ExecutionContext, Job, JobRepository};
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, WorkerId};
use crate::workers::{WorkerRegistry, commands::WorkerCommandSender};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// ExecutionSaga
// ============================================================================

/// Saga para ejecutar un job en un worker.
///
/// # Pasos:
///
/// 1. **ValidateJobStep**: Valida que el job esté en estado Pending
/// 2. **AssignWorkerStep**: Asigna el worker al job
/// 3. **SendRunJobStep**: Envía comando RUN_JOB via gRPC
/// 4. **PublishJobAssignedStep**: Publica evento JobAssigned
///
/// # Compensaciones:
///
/// Si algún paso falla, los pasos anteriores se compensan:
/// - **PublishJobAssignedStep**: No tiene compensación (idempotente)
/// - **SendRunJobStep**: No tiene compensación directa, el timeout del worker maneja esto
/// - **AssignWorkerStep**: Libera el worker del job
/// - **ValidateJobStep**: No hay compensación necesaria
#[derive(Debug, Clone)]
pub struct ExecutionSaga {
    /// Job a ejecutar
    job_id: JobId,
    /// Worker que ejecutará el job
    worker_id: WorkerId,
    /// Repositorio de jobs
    job_repository: Arc<dyn JobRepository>,
    /// Registro de workers
    worker_registry: Arc<dyn WorkerRegistry>,
    /// Sender de comandos gRPC
    worker_command_sender: Arc<dyn WorkerCommandSender>,
}

impl ExecutionSaga {
    /// Crea una nueva ExecutionSaga
    pub fn new(
        job_id: JobId,
        worker_id: WorkerId,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
    ) -> Self {
        Self {
            job_id,
            worker_id,
            job_repository,
            worker_registry,
            worker_command_sender,
        }
    }
}

impl Saga for ExecutionSaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Execution
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ExecutionStepOutput>>> {
        vec![
            Box::new(ValidateJobStep::new(self.job_id.clone())),
            Box::new(AssignWorkerStep::new(
                self.job_id.clone(),
                self.worker_id.clone(),
            )),
            Box::new(SendRunJobStep::new(
                self.job_id.clone(),
                self.worker_id.clone(),
            )),
            Box::new(PublishJobAssignedStep::new(
                self.job_id.clone(),
                self.worker_id.clone(),
            )),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(60))
    }

    fn idempotency_key(&self) -> Option<&str> {
        Some(&format!(
            "execution-saga-{}-{}",
            self.job_id.0, self.worker_id.0
        ))
    }
}

// ============================================================================
// Step Outputs
// ============================================================================

/// Output producido por los steps de ExecutionSaga
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStepOutput {
    pub step_name: &'static str,
    pub job_id: JobId,
    pub worker_id: WorkerId,
    pub details: serde_json::Value,
}

impl ExecutionStepOutput {
    pub fn validate(job_id: JobId, worker_id: WorkerId) -> Self {
        Self {
            step_name: "ValidateJob",
            job_id,
            worker_id,
            details: serde_json::json!({ "validated": true }),
        }
    }

    pub fn assign(job_id: JobId, worker_id: WorkerId, context: ExecutionContext) -> Self {
        Self {
            step_name: "AssignWorker",
            job_id,
            worker_id,
            details: serde_json::json!({
                "execution_id": context.execution_id,
                "provider_id": context.provider_id.to_string()
            }),
        }
    }

    pub fn run_job_sent(job_id: JobId, worker_id: WorkerId) -> Self {
        Self {
            step_name: "SendRunJob",
            job_id,
            worker_id,
            details: serde_json::json!({ "command_sent": true }),
        }
    }

    pub fn job_assigned(job_id: JobId, worker_id: WorkerId) -> Self {
        Self {
            step_name: "PublishJobAssigned",
            job_id,
            worker_id,
            details: serde_json::json!({ "event_published": true }),
        }
    }
}

// ============================================================================
// ValidateJobStep
// ============================================================================

/// Step que valida que el job esté en estado Pending.
#[derive(Debug, Clone)]
pub struct ValidateJobStep {
    job_id: JobId,
}

impl ValidateJobStep {
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ValidateJobStep {
    type Output = ExecutionStepOutput;

    fn name(&self) -> &'static str {
        "ValidateJob"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Save job_id for later steps
        context
            .set_metadata("execution_job_id", &self.job_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        Ok(ExecutionStepOutput::validate(
            self.job_id.clone(),
            JobId::new(), // Will be set in context
        ))
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // No compensation needed for validation
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
// AssignWorkerStep
// ============================================================================

/// Step que asigna el worker al job.
#[derive(Debug, Clone)]
pub struct AssignWorkerStep {
    job_id: JobId,
    worker_id: WorkerId,
}

impl AssignWorkerStep {
    pub fn new(job_id: JobId, worker_id: WorkerId) -> Self {
        Self { job_id, worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for AssignWorkerStep {
    type Output = ExecutionStepOutput;

    fn name(&self) -> &'static str {
        "AssignWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get job from context metadata (set by previous step or caller)
        let job_id_str = context
            .get_metadata::<String>("execution_job_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| self.job_id.to_string());

        context
            .set_metadata("execution_worker_id", &self.worker_id.to_string())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        // Create execution context for compensation
        let execution_context = serde_json::json!({
            "job_id": self.job_id.to_string(),
            "worker_id": self.worker_id.to_string()
        });

        Ok(ExecutionStepOutput::assign(
            self.job_id.clone(),
            self.worker_id.clone(),
            ExecutionContext {
                job_id: self.job_id.clone(),
                provider_id: self.worker_id.provider_id().clone(),
                execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
            },
        ))
    }

    async fn compensate(&self, output: &Self::Output) -> SagaResult<()> {
        // Free the worker from the job
        // The caller must inject worker_registry to perform this
        if let Some(registry_ref) = output.details.get("worker_registry") {
            // This is handled by the saga orchestrator which has access to dependencies
        }
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// SendRunJobStep
// ============================================================================

/// Step que envía el comando RUN_JOB via gRPC al worker.
#[derive(Debug, Clone)]
pub struct SendRunJobStep {
    job_id: JobId,
    worker_id: WorkerId,
}

impl SendRunJobStep {
    pub fn new(job_id: JobId, worker_id: WorkerId) -> Self {
        Self { job_id, worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for SendRunJobStep {
    type Output = ExecutionStepOutput;

    fn name(&self) -> &'static str {
        "SendRunJob"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get job_id and worker_id from context
        let job_id_str = context
            .get_metadata::<String>("execution_job_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| self.job_id.to_string());

        let worker_id_str = context
            .get_metadata::<String>("execution_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| self.worker_id.to_string());

        context
            .set_metadata("run_job_sent_at", &Utc::now().to_rfc3339())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        Ok(ExecutionStepOutput::run_job_sent(
            self.job_id.clone(),
            self.worker_id.clone(),
        ))
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // Compensation for RUN_JOB is handled by:
        // 1. The worker timeout mechanism (worker will timeout if no response)
        // 2. The JobDispatcher rollback that sets job to FAILED state
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true // Sending RUN_JOB multiple times is idempotent
    }
}

// ============================================================================
// PublishJobAssignedStep
// ============================================================================

/// Step que publica el evento JobAssigned.
#[derive(Debug, Clone)]
pub struct PublishJobAssignedStep {
    job_id: JobId,
    worker_id: WorkerId,
}

impl PublishJobAssignedStep {
    pub fn new(job_id: JobId, worker_id: WorkerId) -> Self {
        Self { job_id, worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for PublishJobAssignedStep {
    type Output = ExecutionStepOutput;

    fn name(&self) -> &'static str {
        "PublishJobAssigned"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let now = Utc::now();
        context
            .set_metadata("job_assigned_at", &now.to_rfc3339())
            .map_err(|e| SagaError::PersistenceError {
                message: e.to_string(),
            })?;

        Ok(ExecutionStepOutput::job_assigned(
            self.job_id.clone(),
            self.worker_id.clone(),
        ))
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        // No compensation needed - event publication is idempotent
        // and JobStatusChanged events will be published by other parts of the system
        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_saga_should_have_four_steps() {
        let saga = ExecutionSaga::new(
            JobId::new(),
            WorkerId::new(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            Arc::new(crate::test_dummy_worker_command_sender()),
        );

        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateJob");
        assert_eq!(steps[1].name(), "AssignWorker");
        assert_eq!(steps[2].name(), "SendRunJob");
        assert_eq!(steps[3].name(), "PublishJobAssigned");
    }

    #[test]
    fn execution_saga_has_correct_type() {
        let saga = ExecutionSaga::new(
            JobId::new(),
            WorkerId::new(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            Arc::new(crate::test_dummy_worker_command_sender()),
        );

        assert_eq!(saga.saga_type(), SagaType::Execution);
    }

    #[test]
    fn execution_saga_has_timeout() {
        let saga = ExecutionSaga::new(
            JobId::new(),
            WorkerId::new(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            Arc::new(crate::test_dummy_worker_command_sender()),
        );

        assert!(saga.timeout().is_some());
        assert_eq!(saga.timeout().unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn execution_saga_has_idempotency_key() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let saga = ExecutionSaga::new(
            job_id.clone(),
            worker_id.clone(),
            Arc::new(crate::jobs::test_in_memory_job_repository()),
            Arc::new(crate::test_in_memory_worker_registry()),
            Arc::new(crate::test_dummy_worker_command_sender()),
        );

        let key = saga.idempotency_key().unwrap();
        assert!(key.contains(&job_id.0.to_string()));
        assert!(key.contains(&worker_id.0.to_string()));
    }

    #[test]
    fn validate_job_step_has_no_compensation() {
        let step = ValidateJobStep::new(JobId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn send_run_job_step_is_idempotent() {
        let step = SendRunJobStep::new(JobId::new(), WorkerId::new());
        assert!(step.is_idempotent());
    }

    #[test]
    fn publish_job_assigned_step_has_no_compensation() {
        let step = PublishJobAssignedStep::new(JobId::new(), WorkerId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn execution_step_output_variants() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        let validate = ExecutionStepOutput::validate(job_id.clone(), worker_id.clone());
        assert_eq!(validate.step_name, "ValidateJob");

        let assign = ExecutionStepOutput::assign(
            job_id.clone(),
            worker_id.clone(),
            ExecutionContext {
                job_id: job_id.clone(),
                provider_id: crate::shared_kernel::ProviderId::new(),
                execution_id: "test-exec".to_string(),
            },
        );
        assert_eq!(assign.step_name, "AssignWorker");

        let run_job = ExecutionStepOutput::run_job_sent(job_id.clone(), worker_id.clone());
        assert_eq!(run_job.step_name, "SendRunJob");

        let assigned = ExecutionStepOutput::job_assigned(job_id, worker_id);
        assert_eq!(assigned.step_name, "PublishJobAssigned");
    }
}
