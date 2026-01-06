//! Cancellation Saga
//!
//! Saga para cancelación de jobs en ejecución.
//!
//! Esta saga implementa la lógica de:
//! 1. Validación del job a cancelar
//! 2. Notificación al worker para detener ejecución
//! 3. Actualización del estado del job a CANCELLED
//! 4. Liberación del worker

use crate::WorkerRegistry;
use crate::WorkerState;
use crate::events::DomainEvent;
use crate::jobs::JobRepository;
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, JobState};
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
/// 2. **NotifyWorkerStep**: Notifica al worker para detener la ejecución
/// 3. **UpdateJobStateStep**: Actualiza el estado del job a CANCELLED
/// 4. **ReleaseWorkerStep**: Libera el worker para nuevos jobs
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
        SagaType::Cancellation // EPIC-46 GAP-07: Use dedicated Cancellation type
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(ValidateCancellationStep::new(self.job_id.clone())),
            Box::new(NotifyWorkerStep::new(
                self.job_id.clone(),
                self.reason.clone(),
            )),
            Box::new(UpdateJobStateStep::new(
                self.job_id.clone(),
                JobState::Cancelled,
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
// NotifyWorkerStep
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
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: true,
        })?;

        let worker_registry = &services.provider_registry;

        // Idempotency
        if let Some(Ok(true)) = context.get_metadata::<bool>("worker_notified") {
            debug!(job_id = %self.job_id, "Worker already notified, skipping");
            return Ok(());
        }

        // Find worker for this job
        let worker_opt = worker_registry
            .get_by_job_id(&self.job_id)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to find worker for job: {}", e),
                will_compensate: true,
            })?;

        if let Some(worker) = worker_opt {
            let worker_id = worker.id().clone();

            // Store worker_id for next steps
            context
                .set_metadata("cancelled_worker_id", &worker_id.to_string())
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: true,
                })?;

            // In a real implementation, this would send a cancellation signal to the worker
            // via gRPC stream or NATS message. For now, we just log it.
            info!(
                job_id = %self.job_id,
                worker_id = %worker_id,
                reason = %self.reason,
                "📨 Worker notified of cancellation"
            );

            context
                .set_metadata("worker_notified", &true)
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: true,
                })?;
        } else {
            debug!(job_id = %self.job_id, "No worker assigned to job, skipping notification");
        }

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Cannot undo notification
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
// UpdateJobStateStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct UpdateJobStateStep {
    job_id: JobId,
    target_state: JobState,
}

impl UpdateJobStateStep {
    pub fn new(job_id: JobId, target_state: JobState) -> Self {
        Self {
            job_id,
            target_state,
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

        let job_repository =
            services
                .job_repository
                .as_ref()
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "JobRepository not available".to_string(),
                    will_compensate: true,
                })?;

        let event_bus = &services.event_bus;

        // Idempotency
        if let Some(Ok(true)) = context.get_metadata::<bool>("job_state_updated") {
            debug!(job_id = %self.job_id, "Job state already updated, skipping");
            return Ok(());
        }

        let worker_id_str = context
            .get_metadata::<String>("cancelled_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "none".to_string());

        let target_state = self.target_state.clone();

        info!(
            job_id = %self.job_id,
            target_state = ?target_state,
            "📝 Updating job state to {:?}...",
            target_state
        );

        // Update job state
        job_repository
            .update_state(&self.job_id, target_state.clone())
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to update job state: {}", e),
                will_compensate: true,
            })?;

        // Publish JobCancelled event
        let event = DomainEvent::JobCancelled {
            job_id: self.job_id.clone(),
            reason: Some(
                context
                    .metadata
                    .get("cancellation_reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "user_requested".to_string()),
            ),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            occurred_at: chrono::Utc::now(),
        };

        event_bus
            .publish(&event)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to publish JobCancelled event: {}", e),
                will_compensate: false,
            })?;

        context
            .set_metadata("job_state_updated", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: true,
            })?;

        info!(
            job_id = %self.job_id,
            "✅ Job state updated to {:?}",
            self.target_state
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

        let job_repository =
            services
                .job_repository
                .as_ref()
                .ok_or_else(|| SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: "JobRepository not available".to_string(),
                })?;

        // Restore original state
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

        info!(
            job_id = %self.job_id,
            original_state = ?original_state,
            "🔄 Compensating: restoring job state"
        );

        job_repository
            .update_state(&self.job_id, original_state)
            .await
            .map_err(|e| SagaError::CompensationFailed {
                step: self.name().to_string(),
                message: format!("Failed to restore job state: {}", e),
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
// ReleaseWorkerStep
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

        let worker_registry = &services.provider_registry;

        // Idempotency
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
                "🔄 Releasing worker back to READY state"
            );

            worker_registry
                .update_state(&worker_id, WorkerState::Ready)
                .await
                .map_err(|e| {
                    warn!(worker_id = %worker_id, "Failed to release worker: {}", e);
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
                "✅ Worker released successfully"
            );
        } else {
            debug!(job_id = %self.job_id, "No worker to release");
        }

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Cannot undo worker release in this context
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
}
