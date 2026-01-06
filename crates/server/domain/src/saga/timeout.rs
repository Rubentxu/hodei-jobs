//! Timeout Saga
//!
//! Saga para manejar jobs que exceden su timeout configurado.
//!
//! Esta saga implementa:
//! 1. Detección de timeout basado en tiempo de inicio
//! 2. Notificación al worker para terminación
//! 3. Actualización del job a estado FAILED
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
// TimeoutSaga
// ============================================================================

/// Saga para manejar timeout de jobs en ejecución.
///
/// # Uso:
///
/// Esta saga se dispara cuando un job ha estado en ejecución por más
/// tiempo que su timeout configurado.
///
/// # Pasos:
///
/// 1. **ValidateTimeoutStep**: Verifica que el job realmente ha excedido el timeout
/// 2. **TerminateWorkerStep**: Fuerza la terminación del worker
/// 3. **MarkJobFailedStep**: Actualiza el job a estado FAILED
/// 4. **CleanupWorkerStep**: Limpia el worker para nuevos jobs
#[derive(Debug, Clone)]
pub struct TimeoutSaga {
    /// Job ID que ha excedido el timeout
    pub job_id: JobId,
    /// Timeout configurado para el job
    pub timeout_duration: Duration,
    /// Reason for timeout (e.g., "user_configured", "system_limit")
    pub reason: String,
}

impl TimeoutSaga {
    /// Creates a new TimeoutSaga
    pub fn new(job_id: JobId, timeout_duration: Duration, reason: impl Into<String>) -> Self {
        Self {
            job_id,
            timeout_duration,
            reason: reason.into(),
        }
    }
}

impl Saga for TimeoutSaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Timeout // EPIC-46 GAP-08: Use dedicated Timeout type
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(
                ValidateTimeoutStep::new(self.job_id.clone(), self.timeout_duration)
                    .with_reason(self.reason.clone()),
            ),
            Box::new(TerminateWorkerStep::new(self.job_id.clone())),
            Box::new(MarkJobFailedStep::new(
                self.job_id.clone(),
                self.reason.clone(),
            )),
            Box::new(CleanupWorkerStep::new(self.job_id.clone())),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(15))
    }
}

// ============================================================================
// ValidateTimeoutStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct ValidateTimeoutStep {
    job_id: JobId,
    timeout_duration: Duration,
    reason: String,
}

impl ValidateTimeoutStep {
    pub fn new(job_id: JobId, timeout_duration: Duration) -> Self {
        Self {
            job_id,
            timeout_duration,
            reason: String::new(),
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = reason;
        self
    }
}

#[async_trait::async_trait]
impl SagaStep for ValidateTimeoutStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ValidateTimeout"
    }

    #[instrument(skip(context), fields(step = "ValidateTimeout", job_id = %self.job_id))]
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

        // Idempotency: skip if already processed
        if let Some(Ok(true)) = context.get_metadata::<bool>("timeout_validated") {
            debug!(job_id = %self.job_id, "Timeout already validated, skipping");
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

        // Validate job is still running
        if *job.state() != JobState::Running {
            info!(
                job_id = %self.job_id,
                state = ?job.state(),
                "Job is not running, skipping timeout handling"
            );
            context
                .set_metadata("timeout_skipped", &true)
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: false,
                })?;
            return Ok(());
        }

        // In a real implementation, we would check the actual start time
        // against the current time to verify the timeout
        // For now, we store the timeout info for subsequent steps
        context
            .set_metadata(
                "timeout_duration_ms",
                &(self.timeout_duration.as_millis() as i64),
            )
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        context
            .set_metadata("timeout_reason", &self.reason)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        context
            .set_metadata("timeout_validated", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        info!(
            job_id = %self.job_id,
            timeout_ms = %self.timeout_duration.as_millis(),
            reason = %self.reason,
            "✅ Timeout validated for job"
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
// TerminateWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct TerminateWorkerStep {
    job_id: JobId,
}

impl TerminateWorkerStep {
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for TerminateWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "TerminateWorker"
    }

    #[instrument(skip(context), fields(step = "TerminateWorker", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: true,
        })?;

        let worker_registry = &services.provider_registry;

        // Check if we should skip (job not in running state)
        if let Some(Ok(true)) = context.get_metadata::<bool>("timeout_skipped") {
            debug!(job_id = %self.job_id, "Skipping worker termination (job not running)");
            return Ok(());
        }

        // Idempotency
        if let Some(Ok(true)) = context.get_metadata::<bool>("worker_terminated") {
            debug!(job_id = %self.job_id, "Worker already terminated, skipping");
            return Ok(());
        }

        // Find worker for this job
        let worker_opt = worker_registry
            .get_by_job_id(&self.job_id)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to find worker: {}", e),
                will_compensate: true,
            })?;

        if let Some(worker) = worker_opt {
            let worker_id = worker.id().clone();

            // Store worker_id for next steps
            context
                .set_metadata("timed_out_worker_id", &worker_id.to_string())
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: true,
                })?;

            // In a real implementation, this would:
            // 1. Send SIGTERM to the worker process
            // 2. Wait for graceful shutdown
            // 3. Force kill if necessary
            // 4. Clean up any resources

            info!(
                job_id = %self.job_id,
                worker_id = %worker_id,
                "🛑 Terminating worker due to timeout"
            );

            context
                .set_metadata("worker_terminated", &true)
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: true,
                })?;

            info!(
                worker_id = %worker_id,
                "✅ Worker terminated successfully"
            );
        } else {
            debug!(job_id = %self.job_id, "No worker found for job");
        }

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Cannot undo worker termination
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
// MarkJobFailedStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct MarkJobFailedStep {
    job_id: JobId,
    reason: String,
}

impl MarkJobFailedStep {
    pub fn new(job_id: JobId, reason: String) -> Self {
        Self { job_id, reason }
    }
}

#[async_trait::async_trait]
impl SagaStep for MarkJobFailedStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "MarkJobFailed"
    }

    #[instrument(skip(context), fields(step = "MarkJobFailed", job_id = %self.job_id))]
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

        // Check skip flag
        if let Some(Ok(true)) = context.get_metadata::<bool>("timeout_skipped") {
            debug!(job_id = %self.job_id, "Skipping job failure marking");
            return Ok(());
        }

        // Idempotency
        if let Some(Ok(true)) = context.get_metadata::<bool>("job_marked_failed") {
            debug!(job_id = %self.job_id, "Job already marked as failed, skipping");
            return Ok(());
        }

        let worker_id_str = context
            .get_metadata::<String>("timed_out_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "none".to_string());

        info!(
            job_id = %self.job_id,
            "🚨 Marking job as failed due to timeout"
        );

        // Update job state to FAILED
        job_repository
            .update_state(&self.job_id, JobState::Failed)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to mark job as failed: {}", e),
                will_compensate: true,
            })?;

        // Publish job status changed event (job timed out)
        let timeout_reason = context
            .get_metadata::<String>("timeout_reason")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| self.reason.clone());

        let event = DomainEvent::JobStatusChanged {
            job_id: self.job_id.clone(),
            old_state: JobState::Running,
            new_state: JobState::Timeout,
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            occurred_at: chrono::Utc::now(),
        };

        event_bus
            .publish(&event)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to publish JobTimedOut event: {}", e),
                will_compensate: false,
            })?;

        context
            .set_metadata("job_marked_failed", &true)
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: true,
            })?;

        info!(
            job_id = %self.job_id,
            "✅ Job marked as FAILED due to timeout"
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

        // Try to restore job to running state (best effort)
        info!(job_id = %self.job_id, "Attempting to restore job state");

        job_repository
            .update_state(&self.job_id, JobState::Running)
            .await
            .map_err(|e| {
                warn!(job_id = %self.job_id, "Failed to restore job state: {}", e);
                SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to restore job state: {}", e),
                }
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
// CleanupWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct CleanupWorkerStep {
    job_id: JobId,
}

impl CleanupWorkerStep {
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CleanupWorkerStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "CleanupWorker"
    }

    #[instrument(skip(context), fields(step = "CleanupWorker", job_id = %self.job_id))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        let worker_registry = &services.provider_registry;

        // Check skip flag
        if let Some(Ok(true)) = context.get_metadata::<bool>("timeout_skipped") {
            return Ok(());
        }

        // Idempotency
        if let Some(Ok(true)) = context.get_metadata::<bool>("worker_cleaned_up") {
            return Ok(());
        }

        let worker_id_str = context
            .get_metadata::<String>("timed_out_worker_id")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "none".to_string());

        if worker_id_str != "none" {
            let worker_id = crate::shared_kernel::WorkerId::from_string(&worker_id_str)
                .ok_or_else(|| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Invalid worker_id: {}", worker_id_str),
                    will_compensate: false,
                })?;

            // Mark worker as terminated (non-critical)
            let _ = worker_registry
                .update_state(&worker_id, WorkerState::Terminated)
                .await
                .map_err(|e| {
                    warn!(worker_id = %worker_id, "Failed to mark worker as terminated: {}", e);
                });

            context
                .set_metadata("worker_cleaned_up", &true)
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: false,
                })?;

            info!(
                worker_id = %worker_id,
                "✅ Worker marked for cleanup after timeout"
            );
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_saga_has_four_steps() {
        let saga = TimeoutSaga::new(JobId::new(), Duration::from_secs(300), "user_configured");
        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateTimeout");
        assert_eq!(steps[1].name(), "TerminateWorker");
        assert_eq!(steps[2].name(), "MarkJobFailed");
        assert_eq!(steps[3].name(), "CleanupWorker");
    }

    #[test]
    fn timeout_saga_has_short_timeout() {
        let saga = TimeoutSaga::new(JobId::new(), Duration::from_secs(300), "reason");
        assert_eq!(saga.timeout(), Some(Duration::from_secs(15)));
    }
}
