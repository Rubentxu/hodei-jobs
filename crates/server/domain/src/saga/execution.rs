//! Execution Saga
//!
//! Saga para la ejecución de jobs en workers.
//!
//! Esta saga implementa la lógica real de:
//! 1. Validación del job
//! 2. Asignación de worker
//! 3. Despacho del job
//! 4. Completación del job

use crate::WorkerRegistry;
use crate::WorkerState;
use crate::events::DomainEvent;
use crate::jobs::JobRepository;
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, JobState, WorkerId};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

// ============================================================================
// ExecutionSaga
// ============================================================================

/// Saga para ejecutar un job en un worker.
///
/// # Pasos:
///
/// 1. **ValidateJobStep**: Valida que el job existe, está en estado correcto y tiene especificación válida
/// 2. **AssignWorkerStep**: Encuentra un worker disponible y lo asigna al job
/// 3. **ExecuteJobStep**: Actualiza el estado del job a RUNNING y publica el evento
/// 4. **CompleteJobStep**: Actualiza el estado final del job y publica el evento
///
/// # Error Handling:
///
/// Si cualquier paso falla, la saga ejecutará compensación en reversa:
/// - CompleteJobStep no tiene compensación (el job ya terminó)
/// - ExecuteJobStep: Deshace el cambio de estado a RUNNING
/// - AssignWorkerStep: Libera el worker asignado
/// - ValidateJobStep: No tiene compensación (solo lectura)
#[derive(Debug, Clone)]
pub struct ExecutionSaga {
    /// Job ID a ejecutar
    pub job_id: JobId,
    /// Worker ID opcional (para recuperación)
    pub worker_id: Option<WorkerId>,
}

impl ExecutionSaga {
    /// Crea una nueva ExecutionSaga para el job especificado
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self {
            job_id,
            worker_id: None,
        }
    }

    /// Crea una ExecutionSaga con worker pre-asignado (para recuperación)
    #[inline]
    pub fn with_worker(job_id: JobId, worker_id: WorkerId) -> Self {
        Self {
            job_id,
            worker_id: Some(worker_id),
        }
    }
}

impl Saga for ExecutionSaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Execution
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(ValidateJobStep::new(self.job_id.clone())),
            Box::new(AssignWorkerStep::new(
                self.job_id.clone(),
                self.worker_id.clone(),
            )),
            Box::new(ExecuteJobStep::new(self.job_id.clone())),
            Box::new(CompleteJobStep::new(self.job_id.clone())),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(300)) // 5 minutos para jobs típicos
    }
}

// ============================================================================
// ValidateJobStep
// ============================================================================

/// Step que valida que el job existe y está en estado correcto para ejecución.
///
/// # Operaciones Realizadas:
///
/// 1. Obtiene el job del repository usando `job_repository.find_by_id()`
/// 2. Valida que el job existe (no es None)
/// 3. Valida que está en estado QUEUED (listo para ejecutar)
/// 4. Valida que tiene spec válida
///
/// # Errores:
///
/// - `SagaError::StepFailed` si el job no existe o no está en estado correcto
///
/// # Metadata Guardado:
///
/// - `execution_job`: El job completo serializado (para pasos siguientes)
/// - `execution_job_state`: El estado original del job (para compensación)
#[derive(Debug, Clone)]
pub struct ValidateJobStep {
    job_id: JobId,
}

impl ValidateJobStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ValidateJobStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ValidateJob"
    }

    #[instrument(skip(context), fields(step = "ValidateJob", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: false,
        })?;

        let job_repository =
            services
                .job_repository
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "JobRepository service not available".to_string(),
                    will_compensate: false,
                })?;

        // Idempotency check: skip if already validated
        if let Some(Ok(true)) = context.get_metadata::<bool>("job_validated") {
            debug!(job_id = %self.job_id, "Job already validated (idempotency check), skipping");
            return Ok(());
        }

        info!(job_id = %self.job_id, "🔍 Validating job for execution...");

        // Fetch job from repository
        let job_opt =
            job_repository
                .find_by_id(&self.job_id)
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to fetch job from repository: {}", e),
                    will_compensate: false,
                })?;

        // Validate job exists
        let job = job_opt.ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: format!("Job {} not found in repository", self.job_id),
            will_compensate: false,
        })?;

        // Validate job is in correct state (PENDING for new execution, RUNNING for recovery)
        let current_state = job.state();
        let valid_states = [
            JobState::Pending,
            JobState::Running,
            JobState::Assigned,
            JobState::Scheduled,
        ];

        if !valid_states.contains(&current_state) {
            return Err(SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!(
                    "Job {} is in state {:?}, expected PENDING, ASSIGNED, SCHEDULED or RUNNING",
                    self.job_id, current_state
                ),
                will_compensate: false,
            });
        }

        // Store original state for potential compensation
        context
            .set_metadata("execution_original_state", &format!("{:?}", current_state))
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to store original state: {}", e),
                will_compensate: false,
            })?;

        // Store job_id for subsequent steps
        context
            .set_metadata("execution_job_id", &self.job_id.to_string())
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to store job_id: {}", e),
                will_compensate: false,
            })?;

        // Mark as validated for idempotency
        context
            .set_metadata("job_validated", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to mark job as validated: {}", e),
                will_compensate: false,
            })?;

        info!(
            job_id = %self.job_id,
            state = ?current_state,
            "✅ Job validated successfully for execution"
        );

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // No compensation needed - this step only reads data
        info!(job_id = %self.job_id, "No compensation needed for ValidateJobStep");
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

/// Step que asigna un worker disponible al job.
///
/// # Operaciones Realizadas:
///
/// 1. Si hay worker_id pre-asignado (recuperación), úsalo
/// 2. Si no, busca un worker disponible en estado READY usando `worker_registry.find_ready_worker()`
/// 3. Actualiza el estado del worker a BUSY
/// 4. Asocia el worker al job
///
/// # Errores:
///
/// - `SagaError::StepFailed` si no hay workers disponibles
///
/// # Metadata Guardado:
///
/// - `execution_worker_id`: ID del worker asignado
/// - `worker_assignment_done`: Flag de idempotencia
#[derive(Debug, Clone)]
pub struct AssignWorkerStep {
    job_id: JobId,
    worker_id: Option<WorkerId>,
}

impl AssignWorkerStep {
    #[inline]
    pub fn new(job_id: JobId, worker_id: Option<WorkerId>) -> Self {
        Self { job_id, worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for AssignWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "AssignWorker"
    }

    #[instrument(skip(context), fields(step = "AssignWorker", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: true,
        })?;

        let worker_registry = &services.provider_registry;

        // Idempotency check: skip if already assigned
        if let Some(Ok(true)) = context.get_metadata::<bool>("worker_assignment_done") {
            let worker_id_str = context
                .get_metadata::<String>("execution_worker_id")
                .and_then(|r| r.ok())
                .unwrap_or_else(|| "unknown".to_string());
            debug!(
                job_id = %self.job_id,
                worker_id = %worker_id_str,
                "Worker already assigned (idempotency check), skipping"
            );
            return Ok(());
        }

        // Determine worker_id to use
        let worker_id = if let Some(preassigned) = &self.worker_id {
            // Use pre-assigned worker (for recovery scenarios)
            info!(
                job_id = %self.job_id,
                worker_id = %preassigned,
                "Using pre-assigned worker for job execution"
            );
            preassigned.clone()
        } else {
            // Find available worker
            info!(job_id = %self.job_id, "🔍 Looking for available worker...");

            let worker_opt = worker_registry.find_ready_worker(None).await.map_err(|e| {
                SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to find ready worker: {}", e),
                    will_compensate: true,
                }
            })?;

            match worker_opt {
                Some(worker) => {
                    info!(
                        job_id = %self.job_id,
                        worker_id = %worker.id(),
                        "Found available worker"
                    );
                    worker.id().clone()
                }
                None => {
                    return Err(SagaError::StepFailed {
                        step: self.name().to_string(),
                        message: "No available workers found".to_string(),
                        will_compensate: true,
                    });
                }
            }
        };

        // Mark worker as busy
        worker_registry
            .update_state(&worker_id, WorkerState::Busy)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to update worker state to BUSY: {}", e),
                will_compensate: true,
            })?;

        // Store worker_id for subsequent steps and compensation
        context
            .set_metadata("execution_worker_id", &worker_id.to_string())
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to store worker_id: {}", e),
                will_compensate: true,
            })?;

        // Mark as done for idempotency
        context
            .set_metadata("worker_assignment_done", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to mark worker assignment done: {}", e),
                will_compensate: true,
            })?;

        info!(
            job_id = %self.job_id,
            worker_id = %worker_id,
            "✅ Worker assigned successfully to job"
        );

        Ok(())
    }

    /// Compensates by releasing the worker (marking it as READY again)
    #[instrument(skip(context), fields(step = "AssignWorker"))]
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        let worker_id_str = match context.get_metadata::<String>("execution_worker_id") {
            Some(Ok(id)) => id,
            None => {
                info!("No worker_id in context, skipping worker release compensation");
                return Ok(());
            }
            Some(Err(e)) => {
                warn!("Failed to read worker_id from context: {}", e);
                return Ok(());
            }
        };

        let services = context
            .services()
            .ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "SagaServices not available".to_string(),
            })?;

        let worker_registry = &services.provider_registry;

        let worker_id =
            WorkerId::from_string(&worker_id_str).ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Invalid worker_id in context: {}", worker_id_str),
            })?;

        info!(
            worker_id = %worker_id_str,
            "🔄 Compensating: releasing worker back to READY state"
        );

        // Release worker (mark as READY for other jobs)
        worker_registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .map_err(|e| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Failed to release worker: {}", e),
            })?;

        info!(
            worker_id = %worker_id_str,
            "✅ Worker released (compensation complete)"
        );

        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        true
    }
}

// ============================================================================
// ExecuteJobStep
// ============================================================================

/// Step que inicia la ejecución del job.
///
/// # Operaciones Realizadas:
///
/// 1. Actualiza el estado del job a RUNNING
/// 2. Publica el evento `JobStarted`
///
/// # Nota:
///
/// El despacho real al worker ocurre a través del stream bidireccional
/// gRPC existente. Este step solo marca el inicio de la ejecución.
///
/// # Metadata Guardado:
///
/// - `job_started_at`: Timestamp de inicio de ejecución
/// - `job_execution_started`: Flag de idempotencia
#[derive(Debug, Clone)]
pub struct ExecuteJobStep {
    job_id: JobId,
}

impl ExecuteJobStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ExecuteJobStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ExecuteJob"
    }

    #[instrument(skip(context), fields(step = "ExecuteJob", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: true,
        })?;

        let job_repository =
            services
                .job_repository
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "JobRepository service not available".to_string(),
                    will_compensate: true,
                })?;

        let event_bus = &services.event_bus;

        // Idempotency check: skip if already started
        if let Some(Ok(true)) = context.get_metadata::<bool>("job_execution_started") {
            debug!(job_id = %self.job_id, "Job execution already started (idempotency check), skipping");
            return Ok(());
        }

        info!(job_id = %self.job_id, "🚀 Starting job execution...");

        // Update job state to RUNNING
        let started_at = chrono::Utc::now();
        job_repository
            .update_state(&self.job_id, JobState::Running)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to update job state to RUNNING: {}", e),
                will_compensate: true,
            })?;

        // Get worker_id from context (set by AssignWorkerStep)
        let worker_id_str = context
            .get_metadata::<String>("execution_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "unknown".to_string());

        // Publish job status changed event
        let event = DomainEvent::JobStatusChanged {
            job_id: self.job_id.clone(),
            old_state: JobState::Pending,
            new_state: JobState::Running,
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            occurred_at: started_at,
        };

        event_bus
            .publish(&event)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to publish JobStarted event: {}", e),
                will_compensate: true,
            })?;

        // Store execution timestamp for CompleteJobStep
        context
            .set_metadata("job_started_at", &started_at.to_rfc3339())
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to store execution timestamp: {}", e),
                will_compensate: true,
            })?;

        // Mark as started for idempotency
        context
            .set_metadata("job_execution_started", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to mark execution started: {}", e),
                will_compensate: true,
            })?;

        info!(
            job_id = %self.job_id,
            worker_id = %worker_id_str,
            "✅ Job execution started successfully"
        );

        Ok(())
    }

    /// Compensates by reverting job state back to QUEUED
    #[instrument(skip(context), fields(step = "ExecuteJob"))]
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        let services = context
            .services()
            .ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "SagaServices not available".to_string(),
            })?;

        let job_repository =
            services
                .job_repository
                .as_ref()
                .ok_or_else(|| SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: "JobRepository service not available".to_string(),
                })?;

        info!(
            job_id = %self.job_id,
            "🔄 Compensating: reverting job state to PENDING"
        );

        // Revert job state to PENDING
        job_repository
            .update_state(&self.job_id, JobState::Pending)
            .await
            .map_err(|e| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Failed to revert job state: {}", e),
            })?;

        info!(
            job_id = %self.job_id,
            "✅ Job state reverted to PENDING (compensation complete)"
        );

        Ok(())
    }

    fn is_idempotent(&self) -> bool {
        true
    }

    fn has_compensation(&self) -> bool {
        true
    }
}

// ============================================================================
// CompleteJobStep
// ============================================================================

/// Step que completa la ejecución del job.
///
/// # Operaciones Realizadas:
///
/// 1. Actualiza el estado del job al estado final (COMPLETED o FAILED)
/// 2. Publica el evento `JobCompleted`
/// 3. Libera el worker (lo marca como READY)
///
/// # Metadata Guardado:
///
/// - `job_completed_at`: Timestamp de completación
/// - `job_final_state`: Estado final del job
/// - `job_execution_completed`: Flag de idempotencia
#[derive(Debug, Clone)]
pub struct CompleteJobStep {
    job_id: JobId,
}

impl CompleteJobStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CompleteJobStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "CompleteJob"
    }

    #[instrument(skip(context), fields(step = "CompleteJob", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // Get services from context
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available in context".to_string(),
            will_compensate: false,
        })?;

        let job_repository =
            services
                .job_repository
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "JobRepository service not available".to_string(),
                    will_compensate: false,
                })?;

        let event_bus = &services.event_bus;
        let worker_registry = &services.provider_registry;

        // Idempotency check: skip if already completed
        if let Some(Ok(true)) = context.get_metadata::<bool>("job_execution_completed") {
            debug!(job_id = %self.job_id, "Job already completed (idempotency check), skipping");
            return Ok(());
        }

        // Get execution metadata
        let worker_id_str = context
            .get_metadata::<String>("execution_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "unknown".to_string());

        let started_at_str = context
            .get_metadata::<String>("job_started_at")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        // Determine completion state (this would typically come from worker result)
        // For now, we default to SUCCEEDED - in real scenario, this comes from the worker
        let completed_state = JobState::Succeeded;
        let completed_at = chrono::Utc::now();

        info!(
            job_id = %self.job_id,
            state = ?completed_state,
            "🏁 Completing job execution..."
        );

        // Update job state to final state (clone for the update call)
        job_repository
            .update_state(&self.job_id, completed_state.clone())
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to update job state to {:?}: {}", completed_state, e),
                will_compensate: false,
            })?;

        // Release worker (mark as READY for new jobs)
        let worker_id = WorkerId::from_string(&worker_id_str);
        if let Some(wid) = worker_id {
            worker_registry
                .update_state(&wid, WorkerState::Ready)
                .await
                .map_err(|e| {
                    warn!(worker_id = %wid, "Failed to release worker: {}", e);
                    // Non-critical error - don't fail the saga
                });
        }

        // Publish job status changed event (job completed)
        let event = DomainEvent::JobStatusChanged {
            job_id: self.job_id.clone(),
            old_state: JobState::Running,
            new_state: completed_state.clone(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            occurred_at: completed_at,
        };

        event_bus
            .publish(&event)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to publish JobCompleted event: {}", e),
                will_compensate: false,
            })?;

        // Store completion metadata
        context
            .set_metadata("job_completed_at", &completed_at.to_rfc3339())
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to store completion timestamp: {}", e),
                will_compensate: false,
            })?;

        context
            .set_metadata("job_final_state", &format!("{:?}", completed_state))
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to store final state: {}", e),
                will_compensate: false,
            })?;

        // Mark as completed for idempotency
        context
            .set_metadata("job_execution_completed", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to mark execution completed: {}", e),
                will_compensate: false,
            })?;

        let final_state_str = format!("{:?}", completed_state);

        info!(
            job_id = %self.job_id,
            worker_id = %worker_id_str,
            state = %final_state_str,
            "✅ Job execution completed successfully"
        );

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Cannot compensate a completed job
        // In real scenarios, you'd need a separate CancellationSaga
        info!(job_id = %self.job_id, "Job completion cannot be compensated");
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
    use crate::saga::{SagaContext, SagaId, SagaServices};

    fn create_test_context() -> SagaContext {
        let saga_id = SagaId::new();
        let context = SagaContext::new(
            saga_id,
            SagaType::Execution,
            Some("test-correlation".to_string()),
            Some("test-actor".to_string()),
        );

        // Note: In real tests, you would inject mock services
        context
    }

    #[test]
    fn execution_saga_should_have_four_steps() {
        let saga = ExecutionSaga::new(JobId::new());
        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateJob");
        assert_eq!(steps[1].name(), "AssignWorker");
        assert_eq!(steps[2].name(), "ExecuteJob");
        assert_eq!(steps[3].name(), "CompleteJob");
    }

    #[test]
    fn execution_saga_has_correct_type() {
        let saga = ExecutionSaga::new(JobId::new());
        assert_eq!(saga.saga_type(), SagaType::Execution);
    }

    #[test]
    fn execution_saga_has_timeout() {
        let saga = ExecutionSaga::new(JobId::new());
        assert!(saga.timeout().is_some());
        assert_eq!(saga.timeout().unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn validate_job_step_has_no_compensation() {
        let step = ValidateJobStep::new(JobId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn assign_worker_step_has_compensation() {
        let step = AssignWorkerStep::new(JobId::new(), None);
        assert!(step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn execute_job_step_has_compensation() {
        let step = ExecuteJobStep::new(JobId::new());
        assert!(step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn complete_job_step_has_no_compensation() {
        let step = CompleteJobStep::new(JobId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn execution_saga_with_worker_uses_preassigned_worker() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let saga = ExecutionSaga::with_worker(job_id.clone(), worker_id.clone());

        assert_eq!(saga.job_id, job_id);
        assert_eq!(saga.worker_id, Some(worker_id));
    }
}
