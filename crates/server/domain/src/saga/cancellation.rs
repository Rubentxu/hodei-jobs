//! Cancellation Saga
//!
//! Saga para cancelación de jobs en ejecución.
//!
//! Esta saga implementa la lógica de:
//! 1. Validación del job a cancelar
//! 2. Notificación al worker para detener ejecución (via CommandBus)
//! 3. Actualización del estado del job a CANCELLED (via CommandBus)
//! 4. Liberación del worker (via CommandBus)
//!
//! # Command Bus Pattern
//!
//! Esta saga utiliza el patrón Command Bus para todas las operaciones de dominio,
//! lo que garantiza idempotencia, trazabilidad y potencial persistencia en outbox.

use crate::command::erased::dispatch_erased;
use crate::events::DomainEvent;
use crate::jobs::events::JobCancelled;
use crate::jobs::JobRepository;
use crate::saga::commands::cancellation::{
    NotifyWorkerCommand, ReleaseWorkerCommand, UpdateJobStateCommand,
};
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, JobState};
use crate::workers::WorkerRegistry;
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

// ============================================================================
// CancellationSaga
// ============================================================================

/// Saga para cancelar un job en ejecución.
///
/// # Pasos:
///
/// 1. **ValidateCancellationStep**: Valida que el job existe y está en estado ejecutable
/// 2. **NotifyWorkerStep**: Notifica al worker para detener la ejecución (via CommandBus)
/// 3. **UpdateJobStateStep**: Actualiza el estado del job a CANCELLED (via CommandBus)
/// 4. **ReleaseWorkerStep**: Libera el worker para nuevos jobs (via CommandBus)
///
/// # Metadata Guardado:
///
/// - `cancellation_reason`: Razón de la cancelación
/// - `cancelled_worker_id`: Worker que estaba ejecutando el job
#[derive(Debug, Clone)]
pub struct CancellationSaga {
    /// Job ID a cancelar
    pub job_id: JobId,
    /// Razón de cancelación
    pub reason: String,
}

impl CancellationSaga {
    /// Crea una nueva CancellationSaga
    pub fn new(job_id: JobId, reason: impl Into<String>) -> Self {
        Self {
            job_id,
            reason: reason.into(),
        }
    }
}

impl Saga for CancellationSaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Cancellation
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(ValidateCancellationStep::new(self.job_id.clone())),
            Box::new(NotifyWorkerStep::new(
                self.job_id.clone(),
                self.reason.clone(),
            )),
            Box::new(UpdateJobStateStep::cancel(
                self.job_id.clone(),
                self.reason.clone(),
            )),
            Box::new(ReleaseWorkerStep::new(self.job_id.clone())),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(30))
    }

    fn idempotency_key(&self) -> Option<&str> {
        Some(&self.reason)
    }
}

// ============================================================================
// ValidateCancellationStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct ValidateCancellationStep {
    job_id: JobId,
}

impl ValidateCancellationStep {
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ValidateCancellationStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ValidateCancellation"
    }

    #[instrument(skip(context), fields(step = "ValidateCancellation", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        let job_repository =
            services
                .job_repository
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "JobRepository not available".to_string(),
                    will_compensate: false,
                })?;

        // Idempotency: skip if already cancelled
        if let Some(Ok(true)) = context.get_metadata::<bool>("cancellation_validated") {
            debug!(job_id = %self.job_id, "Cancellation already validated, skipping");
            return Ok(());
        }

        // Fetch job
        let job_opt =
            job_repository
                .find_by_id(&self.job_id)
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to fetch job: {}", e),
                    will_compensate: false,
                })?;

        let job = job_opt.ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: format!("Job {} not found", self.job_id),
            will_compensate: false,
        })?;

        // Validate job is in cancellable state
        let cancellable_states = [
            JobState::Pending,
            JobState::Assigned,
            JobState::Running,
            JobState::Scheduled,
        ];
        if !cancellable_states.contains(&job.state()) {
            return Err(SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!(
                    "Job {} is in state {:?}, cannot cancel",
                    self.job_id,
                    job.state()
                ),
                will_compensate: false,
            });
        }

        // Store original state for potential rollback
        context
            .set_metadata("cancellation_original_state", &format!("{:?}", job.state()))
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        context
            .set_metadata("cancellation_validated", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        info!(
            job_id = %self.job_id,
            state = ?job.state(),
            "✅ Cancellation validated"
        );

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
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
// NotifyWorkerStep - USANDO COMMANDBUS (GAP-52-02)
// ============================================================================

#[derive(Debug, Clone)]
pub struct NotifyWorkerStep {
    job_id: JobId,
    reason: String,
}

impl NotifyWorkerStep {
    pub fn new(job_id: JobId, reason: String) -> Self {
        Self { job_id, reason }
    }
}

#[async_trait::async_trait]
impl SagaStep for NotifyWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "NotifyWorker"
    }

    #[instrument(skip(context), fields(step = "NotifyWorker", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        // GAP-52-02: Extract services data first to avoid borrow conflict
        let (command_bus, saga_id_str, worker_id_opt) = {
            let services = context.services().ok_or_else(|| SagaError::StepFailed {
                step: self.name().to_string(),
                message: "SagaServices not available".to_string(),
                will_compensate: true,
            })?;

            let command_bus = services.command_bus.as_ref().ok_or_else(|| {
                SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "CommandBus not available in SagaServices".to_string(),
                    will_compensate: true,
                }
            })?;

            let worker_registry = &services.provider_registry;

            // Idempotency check stored by CommandBus automatically
            if let Some(Ok(true)) = context.get_metadata::<bool>("worker_notified") {
                debug!(job_id = %self.job_id, "Worker already notified, skipping");
                return Ok(());
            }

            // Find worker for this job (needed for the command)
            let worker_opt = worker_registry
                .get_by_job_id(&self.job_id)
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to find worker for job: {}", e),
                    will_compensate: true,
                })?;

            // Clone values to drop the borrow on services
            (
                command_bus.clone(),
                context.saga_id.to_string(),
                worker_opt.and_then(|w| Some(w.id().clone())),
            )
        };

        // Store worker_id for next steps (now that services borrow is dropped)
        if let Some(ref worker_id) = worker_id_opt {
            context
                .set_metadata("cancelled_worker_id", &worker_id.to_string())
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: true,
                })?;
        }

        // GAP-52-02: Dispatch command via CommandBus
        if let Some(worker_id) = worker_id_opt {
            let command = NotifyWorkerCommand::new(
                self.job_id.clone(),
                Some(worker_id),
                self.reason.clone(),
                saga_id_str,
            );

            dispatch_erased(&command_bus, command)
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to notify worker: {}", e),
                    will_compensate: true,
                })?;
        } else {
            debug!(job_id = %self.job_id, "No worker assigned to job, skipping notification");
        }

        context
            .set_metadata("worker_notified", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: true,
            })?;

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Cannot undo notification - it's a best-effort signal
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
// UpdateJobStateStep - USANDO COMMANDBUS (GAP-52-02)
// ============================================================================

#[derive(Debug, Clone)]
pub struct UpdateJobStateStep {
    job_id: JobId,
    target_state: JobState,
    reason: String,
    saga_id: String,
}

impl UpdateJobStateStep {
    /// Creates a step to update job state to cancelled
    pub fn cancel(job_id: JobId, reason: String) -> Self {
        Self {
            job_id,
            target_state: JobState::Cancelled,
            reason,
            saga_id: String::new(),
        }
    }

    /// Creates a step with custom target state
    pub fn with_state(job_id: JobId, target_state: JobState, saga_id: String) -> Self {
        Self {
            job_id,
            target_state,
            reason: String::new(),
            saga_id,
        }
    }
}

#[async_trait::async_trait]
impl SagaStep for UpdateJobStateStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "UpdateJobState"
    }

    #[instrument(skip(context), fields(step = "UpdateJobState", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: true,
        })?;

        // GAP-52-02: Get CommandBus from services
        let command_bus = services.command_bus.as_ref().ok_or_else(|| {
            SagaError::StepFailed {
                step: self.name().to_string(),
                message: "CommandBus not available in SagaServices".to_string(),
                will_compensate: true,
            }
        })?;

        let event_bus = &services.event_bus;

        // Idempotency check stored by CommandBus automatically
        if let Some(Ok(true)) = context.get_metadata::<bool>("job_state_updated") {
            debug!(job_id = %self.job_id, "Job state already updated, skipping");
            return Ok(());
        }

        let target_state = self.target_state.clone();

        info!(
            job_id = %self.job_id,
            target_state = ?target_state,
            "📝 Updating job state via CommandBus to {:?}...",
            target_state
        );

        // GAP-52-02: Dispatch command via CommandBus
        let command = UpdateJobStateCommand::cancel(
            self.job_id.clone(),
            context.saga_id.to_string(),
        );

        let result = dispatch_erased(command_bus, command)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to update job state via CommandBus: {}", e),
                will_compensate: true,
            })?;

        // If state was already correct (idempotent skip), we still publish event
        if !result.already_in_state {
            // Publish JobCancelled event
            // EPIC-65 Phase 3: Using modular event type
            let cancelled_event = JobCancelled {
                job_id: self.job_id.clone(),
                reason: Some(self.reason.clone()),
                correlation_id: context.correlation_id.clone(),
                actor: context.actor.clone(),
                occurred_at: chrono::Utc::now(),
            };

            event_bus
                .publish(&cancelled_event.into())
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to publish JobCancelled event: {}", e),
                    will_compensate: false,
                })?;
        }

        context
            .set_metadata("job_state_updated", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: true,
            })?;

        info!(
            job_id = %self.job_id,
            new_state = ?result.new_state,
            already_in_state = result.already_in_state,
            "✅ Job state updated via CommandBus"
        );

        Ok(())
    }

    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        let services = context
            .services()
            .ok_or_else(|| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "SagaServices not available".to_string(),
            })?;

        // GAP-52-02: Get CommandBus for compensation
        let command_bus = services.command_bus.as_ref().ok_or_else(|| {
            SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: "CommandBus not available in SagaServices".to_string(),
            }
        })?;

        // Restore original state via CommandBus
        let original_state_str = context
            .get_metadata::<String>("cancellation_original_state")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "Queued".to_string());

        let original_state = match original_state_str.as_str() {
            "Running" => JobState::Running,
            "Pending" => JobState::Pending,
            "Assigned" => JobState::Assigned,
            "Scheduled" => JobState::Scheduled,
            _ => JobState::Pending,
        };
        let original_state_for_log = original_state.clone();
        let original_state_for_command = original_state.clone();

        info!(
            job_id = %self.job_id,
            original_state = ?original_state_for_log,
            "🔄 Compensating: restoring job state via CommandBus"
        );

        let command = UpdateJobStateCommand::with_original_state(
            self.job_id.clone(),
            original_state,
            original_state_for_command,
            context.saga_id.to_string(),
        );

        dispatch_erased(command_bus, command)
            .await
            .map_err(|e| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Failed to restore job state via CommandBus: {}", e),
            })?;

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
// ReleaseWorkerStep - USANDO COMMANDBUS (GAP-52-02)
// ============================================================================

#[derive(Debug, Clone)]
pub struct ReleaseWorkerStep {
    job_id: JobId,
}

impl ReleaseWorkerStep {
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ReleaseWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ReleaseWorker"
    }

    #[instrument(skip(context), fields(step = "ReleaseWorker", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        // GAP-52-02: Get CommandBus from services
        let command_bus = services.command_bus.as_ref().ok_or_else(|| {
            SagaError::StepFailed {
                step: self.name().to_string(),
                message: "CommandBus not available in SagaServices".to_string(),
                will_compensate: false,
            }
        })?;

        // Idempotency check stored by CommandBus automatically
        if let Some(Ok(true)) = context.get_metadata::<bool>("worker_released") {
            debug!(job_id = %self.job_id, "Worker already released, skipping");
            return Ok(());
        }

        let worker_id_str = context
            .get_metadata::<String>("cancelled_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "none".to_string());

        if worker_id_str != "none" {
            let worker_id = crate::shared_kernel::WorkerId::from_string(&worker_id_str)
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Invalid worker_id: {}", worker_id_str),
                    will_compensate: false,
                })?;

            info!(
                job_id = %self.job_id,
                worker_id = %worker_id,
                "🔄 Releasing worker back to READY state via CommandBus"
            );

            // GAP-52-02: Dispatch command via CommandBus
            let command = ReleaseWorkerCommand::new(worker_id.clone(), context.saga_id.to_string());

            dispatch_erased(command_bus, command)
                .await
                .map_err(|e| {
                    warn!(worker_id = %worker_id, "Failed to release worker via CommandBus: {}", e);
                    // Non-critical, continue silently
                });

            context
                .set_metadata("worker_released", &true)
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: false,
                })?;

            info!(
                worker_id = %worker_id,
                "✅ Worker released successfully via CommandBus"
            );
        } else {
            debug!(job_id = %self.job_id, "No worker to release");
        }

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Cannot undo worker release - it's a state transition
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
// Helper Functions
// ============================================================================

/// Extracts worker ID from optional worker reference
#[inline]
fn worker_id_from_opt(worker_opt: &Option<super::super::workers::Worker>) -> Option<&super::super::WorkerId> {
    worker_opt.as_ref().map(|w| w.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_saga_has_four_steps() {
        let saga = CancellationSaga::new(JobId::new(), "user_requested");
        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateCancellation");
        assert_eq!(steps[1].name(), "NotifyWorker");
        assert_eq!(steps[2].name(), "UpdateJobState");
        assert_eq!(steps[3].name(), "ReleaseWorker");
    }

    #[test]
    fn cancellation_saga_has_correct_timeout() {
        let saga = CancellationSaga::new(JobId::new(), "user_requested");
        assert_eq!(saga.timeout(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn cancellation_saga_has_idempotency_key() {
        let saga = CancellationSaga::new(JobId::new(), "user_requested");
        assert_eq!(saga.idempotency_key(), Some("user_requested"));
    }

    #[test]
    fn update_job_state_step_cancel_creates_cancelled_state() {
        let job_id = JobId::new();
        let step = UpdateJobStateStep::cancel(job_id.clone(), "user_requested".to_string());
        assert_eq!(step.job_id, job_id);
        assert_eq!(step.target_state, JobState::Cancelled);
    }

    #[test]
    fn notify_worker_step_stores_reason() {
        let job_id = JobId::new();
        let step = NotifyWorkerStep::new(job_id.clone(), "timeout".to_string());
        assert_eq!(step.job_id, job_id);
        assert_eq!(step.reason, "timeout");
    }
}
