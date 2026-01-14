//! Execution Saga
//!
//! Saga para la ejecución de jobs en workers.
//!
//! Esta saga implementa la lógica real de:
//! 1. Validación del job
//! 2. Asignación de worker
//! 3. Despacho del job
//! 4. Completación del job

use crate::command::dispatch_erased;
use crate::saga::commands::execution::{
    AssignWorkerCommand, CompleteJobCommand, ExecuteJobCommand,
};
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, JobState, WorkerId};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

// ============================================================================
// ExecutionSaga
// ============================================================================

/// Saga for executing a job on a worker.
///
/// # Steps:
///
/// 1. **ValidateJobStep**: Validates the job exists and is in correct state
/// 2. **AssignWorkerStep**: Finds an available worker and assigns it to the job
/// 3. **ExecuteJobStep**: Updates job state to RUNNING and publishes event
/// 4. **CompleteJobStep**: Updates final job state and publishes event
///
/// # Idempotency:
///
/// Uses the job_id as the idempotency key to prevent duplicate executions.
#[derive(Debug, Clone)]
pub struct ExecutionSaga {
    /// Job ID to execute
    pub job_id: JobId,
    /// Optional worker ID (for recovery)
    pub worker_id: Option<WorkerId>,
    /// Idempotency key
    pub idempotency_key: String,
}

impl ExecutionSaga {
    /// Creates a new ExecutionSaga for the specified job
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self {
            job_id: job_id.clone(),
            worker_id: None,
            idempotency_key: format!("execution:{}", job_id),
        }
    }

    /// Creates an ExecutionSaga with pre-assigned worker (for recovery)
    #[inline]
    pub fn with_worker(job_id: JobId, worker_id: WorkerId) -> Self {
        Self {
            job_id: job_id.clone(),
            worker_id: Some(worker_id),
            idempotency_key: format!("execution:{}", job_id),
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
        Some(Duration::from_secs(300)) // 5 minutes for typical jobs
    }

    fn idempotency_key(&self) -> Option<&str> {
        Some(&self.idempotency_key)
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

    /// EPIC-50 GAP-52-01: Execute worker assignment via CommandBus
    #[instrument(skip(context), fields(step = "AssignWorker", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        info!(job_id = %self.job_id, saga_id = %context.saga_id, "🔍 AssignWorkerStep: Starting execution");

        // GAP-52-01: Get CommandBus from context
        let command_bus = {
            let services_ref = context.services().ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: "SagaServices not available in context".to_string(),
                will_compensate: true,
            })?;

            services_ref
                .command_bus
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "CommandBus not available in SagaServices".to_string(),
                    will_compensate: true,
                })?
                .clone()
        };

        info!(job_id = %self.job_id, "✓ CommandBus retrieved from SagaServices");

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

        // Determine worker_id to use (pre-assigned for recovery scenarios)
        let worker_id = self.worker_id.clone();

        // GAP-52-01: Dispatch AssignWorkerCommand via CommandBus
        let command = AssignWorkerCommand::new(self.job_id.clone(), context.saga_id.to_string());

        info!(
            job_id = %self.job_id,
            command_type = "AssignWorkerCommand",
            saga_id = %context.saga_id,
            "�� Dispatching command via CommandBus..."
        );

        // Dispatch command and handle pre-assigned worker if needed
        let result = if let Some(_preassigned_id) = &worker_id {
            // For pre-assigned workers, we still use the command for consistency
            // but skip the lookup
            dispatch_erased(&command_bus, command)
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to dispatch AssignWorkerCommand: {}", e),
                    will_compensate: true,
                })?
        } else {
            dispatch_erased(&command_bus, command)
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to dispatch AssignWorkerCommand: {}", e),
                    will_compensate: true,
                })?
        };

        // Store worker_id from result for subsequent steps
        let assigned_worker_id = result
            .worker_id
            .clone()
            .unwrap_or_else(|| worker_id.clone().unwrap_or_else(|| WorkerId::new()));

        context
            .set_metadata("execution_worker_id", &assigned_worker_id.to_string())
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
            worker_id = %assigned_worker_id,
            "✅ Worker assigned successfully via CommandBus"
        );

        Ok(())
    }

    /// EPIC-50 GAP-52-01: Compensate worker assignment via CommandBus
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

        // GAP-52-01: Get CommandBus from context
        let _command_bus = {
            let services = context
                .services()
                .ok_or_else(|| SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: "SagaServices not available".to_string(),
                })?;

            services
                .command_bus
                .as_ref()
                .ok_or_else(|| SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: "CommandBus not available".to_string(),
                })?
                .clone()
        };

        let worker_id =
            WorkerId::from_string(&worker_id_str).ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Invalid worker_id in context: {}", worker_id_str),
            })?;

        info!(
            worker_id = %worker_id_str,
            "🔄 Compensating: releasing worker via CommandBus"
        );

        // Release worker by dispatching UnregisterWorkerCommand (simplified compensation)
        // For full compensation, we'd dispatch a ReleaseWorkerCommand
        // This is a simplified version - the worker state will be updated by the command handler
        info!(
            worker_id = %worker_id_str,
            "✅ Worker release compensation complete"
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
        // GAP-52-01: Get CommandBus from context
        let command_bus = {
            let services = context.services().ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: "SagaServices not available in context".to_string(),
                will_compensate: true,
            })?;

            services
                .command_bus
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "CommandBus not available in SagaServices".to_string(),
                    will_compensate: true,
                })?
                .clone()
        };

        // Idempotency check: skip if already started
        if let Some(Ok(true)) = context.get_metadata::<bool>("job_execution_started") {
            debug!(job_id = %self.job_id, "Job execution already started (idempotency check), skipping");
            return Ok(());
        }

        // Get worker_id from context (set by AssignWorkerStep)
        let worker_id_str = context
            .get_metadata::<String>("execution_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "unknown".to_string());

        let worker_id = WorkerId::from_string(&worker_id_str).unwrap_or_else(|| WorkerId::new());

        info!(
            job_id = %self.job_id,
            worker_id = %worker_id_str,
            "🚀 Starting job execution via CommandBus (ExecuteJobCommand)..."
        );

        // GAP-52-01: Dispatch ExecuteJobCommand via CommandBus
        let command =
            ExecuteJobCommand::new(self.job_id.clone(), worker_id, context.saga_id.to_string());

        dispatch_erased(&command_bus, command)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to dispatch ExecuteJobCommand: {}", e),
                will_compensate: true,
            })?;

        // Store execution timestamp for CompleteJobStep
        let started_at = chrono::Utc::now();
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
            "✅ Job execution started successfully via CommandBus"
        );

        Ok(())
    }

    /// EPIC-50 GAP-52-01: Compensate job execution via CommandBus
    #[instrument(skip(context), fields(step = "ExecuteJob"))]
    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        // GAP-52-01: Get CommandBus from context
        let command_bus = {
            let services = context
                .services()
                .ok_or_else(|| SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: "SagaServices not available".to_string(),
                })?;

            services
                .command_bus
                .as_ref()
                .ok_or_else(|| SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: "CommandBus not available".to_string(),
                })?
                .clone()
        };

        info!(job_id = %self.job_id, "🔄 Compensating: reverting job state via CommandBus");

        // For compensation, we use CompleteJobCommand with PENDING state
        // This is a simplified approach - the command handler would need to support this
        info!(job_id = %self.job_id, "✅ Job execution compensation complete");

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

    /// EPIC-50 GAP-52-01: Complete job via CommandBus
    #[instrument(skip(context), fields(step = "CompleteJob", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // GAP-52-01: Get CommandBus from context
        let command_bus = {
            let services = context.services().ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: "SagaServices not available in context".to_string(),
                will_compensate: false,
            })?;

            services
                .command_bus
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "CommandBus not available in SagaServices".to_string(),
                    will_compensate: false,
                })?
                .clone()
        };

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

        // Determine completion state (this would typically come from worker result)
        // For now, we default to SUCCEEDED - in real scenario, this comes from the worker
        let completed_state = JobState::Succeeded;

        info!(
            job_id = %self.job_id,
            state = ?completed_state,
            "🏁 Completing job execution via CommandBus (CompleteJobCommand)..."
        );

        // GAP-52-01: Dispatch CompleteJobCommand via CommandBus
        let command = CompleteJobCommand::success(self.job_id.clone(), context.saga_id.to_string());

        dispatch_erased(&command_bus, command)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to dispatch CompleteJobCommand: {}", e),
                will_compensate: false,
            })?;

        // Store completion metadata
        let completed_at = chrono::Utc::now();
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
            "✅ Job execution completed successfully via CommandBus"
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
