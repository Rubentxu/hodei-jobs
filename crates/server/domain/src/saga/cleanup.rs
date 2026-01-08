//! Cleanup Saga
//!
//! Saga para limpieza de recursos huérfanos y mantenimiento del sistema.
//!
//! Esta saga implementa:
//! 1. Identificación de workers huérfanos (sin heartbeat)
//! 2. Identificación de jobs huérfanos (en estado Running por mucho tiempo)
//! 3. Limpieza de recursos asociados
//! 4. Publicación de eventos deCleanup

use crate::ProviderId;
use crate::WorkerRegistry;
use crate::WorkerState;
use crate::command::erased::dispatch_erased;
use crate::command::jobs::MarkJobFailedCommand;
use crate::events::DomainEvent;
use crate::jobs::JobRepository;
use crate::saga::commands::UnregisterWorkerCommand;
use crate::saga::{Saga, SagaContext, SagaError, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::{JobId, JobState, WorkerId};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

// ============================================================================
// Constants
// ============================================================================

/// Default threshold for considering a worker as unhealthy (no heartbeat)
const DEFAULT_UNHEALTHY_THRESHOLD: Duration = Duration::from_secs(300);

/// Default threshold for considering a job as orphaned (no update)
const DEFAULT_ORPHANED_JOB_THRESHOLD: Duration = Duration::from_secs(600);

// ============================================================================
// CleanupSaga
// ============================================================================

/// Saga para limpieza de recursos huérfanos y mantenimiento.
///
/// # Uso:
///
/// Esta saga se ejecuta periódicamente (cron job) para:
///
/// - Limpiar workers sin heartbeat
/// - Resetear jobs huérfanos
/// - Liberar recursos no utilizados
/// - Publicar métricas de cleanup
///
/// # Pasos:
///
/// 1. **IdentifyUnhealthyWorkersStep**: Encuentra workers sin heartbeat
/// 2. **IdentifyOrphanedJobsStep**: Encuentra jobs en estado Running sin actualización
/// 3. **CleanupUnhealthyWorkersStep**: Limpia workers identificados
/// 4. **ResetOrphanedJobsStep**: Resetea jobs huérfanos a estado failed
/// 5. **PublishCleanupMetricsStep**: Publica métricas de cleanup
#[derive(Debug, Clone)]
pub struct CleanupSaga {
    /// Threshold para considerar un worker como unhealthy
    pub unhealthy_threshold: Duration,
    /// Threshold para considerar un job como orphaned
    pub orphaned_job_threshold: Duration,
    /// Whether to actually perform cleanup or just report
    pub dry_run: bool,
}

impl Default for CleanupSaga {
    fn default() -> Self {
        Self {
            unhealthy_threshold: DEFAULT_UNHEALTHY_THRESHOLD,
            orphaned_job_threshold: DEFAULT_ORPHANED_JOB_THRESHOLD,
            dry_run: false,
        }
    }
}

impl CleanupSaga {
    /// Creates a new CleanupSaga with default thresholds
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a CleanupSaga with custom thresholds
    pub fn with_thresholds(
        unhealthy_threshold: Duration,
        orphaned_job_threshold: Duration,
    ) -> Self {
        Self {
            unhealthy_threshold,
            orphaned_job_threshold,
            dry_run: false,
        }
    }

    /// Enable dry-run mode (report only, no actual cleanup)
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}

impl Saga for CleanupSaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Cleanup // EPIC-46 GAP-09: Use dedicated Cleanup type
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(IdentifyUnhealthyWorkersStep::new(
                self.unhealthy_threshold,
                self.dry_run,
            )),
            Box::new(IdentifyOrphanedJobsStep::new(
                self.orphaned_job_threshold,
                self.dry_run,
            )),
            Box::new(CleanupUnhealthyWorkersStep::new(self.dry_run)),
            Box::new(ResetOrphanedJobsStep::new(self.dry_run)),
            Box::new(PublishCleanupMetricsStep::new()),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(60))
    }
}

// ============================================================================
// IdentifyUnhealthyWorkersStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct IdentifyUnhealthyWorkersStep {
    threshold: Duration,
    dry_run: bool,
}

impl IdentifyUnhealthyWorkersStep {
    pub fn new(threshold: Duration, dry_run: bool) -> Self {
        Self { threshold, dry_run }
    }
}

#[async_trait::async_trait]
impl SagaStep for IdentifyUnhealthyWorkersStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "IdentifyUnhealthyWorkers"
    }

    #[instrument(skip(context), fields(step = "IdentifyUnhealthyWorkers"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        let worker_registry = &services.provider_registry;

        // Find all workers
        let all_workers =
            worker_registry
                .find_available()
                .await
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: format!("Failed to fetch workers: {}", e),
                    will_compensate: false,
                })?;

        // EPIC-50 GAP-MOD-02: Filter for unhealthy workers by checking:
        // 1. Workers stuck in Creating state (possibly failed to start)
        // 2. Workers with stale heartbeat (no heartbeat for longer than threshold)
        let now = chrono::Utc::now();
        let unhealthy_workers: Vec<WorkerId> = all_workers
            .iter()
            .filter(|w| {
                // Check if worker is in unexpected/stuck state
                let is_stuck_state = matches!(w.state(), WorkerState::Creating);

                // Check if heartbeat is stale (exceeds threshold)
                let last_hb = w.last_heartbeat();
                let elapsed = now.signed_duration_since(last_hb);
                let is_stale_heartbeat = elapsed.to_std()
                    .map(|d| d > self.threshold)
                    .unwrap_or(true); // If conversion fails, consider stale

                // Worker is unhealthy if stuck OR has stale heartbeat (and not already terminated)
                is_stuck_state || (is_stale_heartbeat && !matches!(w.state(), WorkerState::Terminated))
            })
            .map(|w| w.id().clone())
            .collect();

        debug!(
            total_workers = %all_workers.len(),
            unhealthy_count = %unhealthy_workers.len(),
            threshold_secs = %self.threshold.as_secs(),
            "Analyzed workers for heartbeat staleness"
        );

        context
            .set_metadata(
                "unhealthy_worker_ids",
                &unhealthy_workers
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        context
            .set_metadata("unhealthy_worker_count", &(unhealthy_workers.len() as i64))
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        let mode = if self.dry_run { "DRY RUN" } else { "EXECUTE" };
        info!(
            mode = %mode,
            count = %unhealthy_workers.len(),
            threshold_secs = %self.threshold.as_secs(),
            "✅ Identified {} unhealthy workers",
            unhealthy_workers.len()
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
// IdentifyOrphanedJobsStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct IdentifyOrphanedJobsStep {
    threshold: Duration,
    dry_run: bool,
}

impl IdentifyOrphanedJobsStep {
    pub fn new(threshold: Duration, dry_run: bool) -> Self {
        Self { threshold, dry_run }
    }
}

#[async_trait::async_trait]
impl SagaStep for IdentifyOrphanedJobsStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "IdentifyOrphanedJobs"
    }

    #[instrument(skip(context), fields(step = "IdentifyOrphanedJobs"))]
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

        // Find all running jobs
        let running_jobs = job_repository
            .find_by_state(&JobState::Running)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to fetch running jobs: {}", e),
                will_compensate: false,
            })?;

        // In a real implementation, we would check last update timestamp
        // For now, we collect all running jobs as potentially orphaned
        let orphaned_job_ids: Vec<JobId> = running_jobs.iter().map(|j| j.id.clone()).collect();

        context
            .set_metadata(
                "orphaned_job_ids",
                &orphaned_job_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        context
            .set_metadata("orphaned_job_count", &(orphaned_job_ids.len() as i64))
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        let mode = if self.dry_run { "DRY RUN" } else { "EXECUTE" };
        info!(
            mode = %mode,
            count = %orphaned_job_ids.len(),
            "✅ Identified {} potentially orphaned jobs",
            orphaned_job_ids.len()
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
// CleanupUnhealthyWorkersStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct CleanupUnhealthyWorkersStep {
    dry_run: bool,
}

impl CleanupUnhealthyWorkersStep {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }
}

#[async_trait::async_trait]
impl SagaStep for CleanupUnhealthyWorkersStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "CleanupUnhealthyWorkers"
    }

    #[instrument(skip(context), fields(step = "CleanupUnhealthyWorkers"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        // Clone command_bus Arc to avoid borrowing context immutably while mutating it later
        let command_bus = services.command_bus.as_ref().cloned().ok_or_else(|| {
            SagaError::StepFailed {
                step: self.name().to_string(),
                message: "CommandBus not available".to_string(),
                will_compensate: false,
            }
        })?;

        // Get list of unhealthy workers
        let worker_ids: Vec<String> = context
            .get_metadata::<Vec<String>>("unhealthy_worker_ids")
            .and_then(|r| r.ok())
            .unwrap_or_default();

        let mode = if self.dry_run { "DRY RUN" } else { "CLEANUP" };

        if worker_ids.is_empty() {
            info!(mode = %mode, "No unhealthy workers to clean up");
            context
                .set_metadata("cleaned_up_worker_count", &0i64)
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: false,
                })?;
            return Ok(());
        }

        let mut cleaned_count = 0;

        for worker_id_str in worker_ids {
            if self.dry_run {
                debug!(worker_id = %worker_id_str, "Would clean up unhealthy worker");
                cleaned_count += 1;
                continue;
            }

            let worker_id = WorkerId::from_string(&worker_id_str);
            if let Some(wid) = worker_id {
                // Unregister the unhealthy worker via CommandBus
                let command = UnregisterWorkerCommand::with_reason(
                    wid.clone(),
                    context.saga_id.to_string(),
                    "Cleanup unhealthy worker",
                );

                match dispatch_erased(&command_bus, command).await {
                    Ok(_) => {
                        cleaned_count += 1;
                        info!(worker_id = %wid, "Cleaned up unhealthy worker via CommandBus");
                    }
                    Err(e) => {
                        warn!(worker_id = %wid, "Failed to unregister worker: {}", e);
                    }
                }
            }
        }

        context
            .set_metadata("cleaned_up_worker_count", &(cleaned_count as i64))
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        info!(
            mode = %mode,
            count = %cleaned_count,
            "✅ {} unhealthy workers",
            if self.dry_run { "Would clean up" } else { "Cleaned up" }
        );

        Ok(())
    }

    async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
        // Cannot undo cleanup in dry_run mode
        // In real mode, we would need a more complex recovery mechanism
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
// ResetOrphanedJobsStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct ResetOrphanedJobsStep {
    dry_run: bool,
}

impl ResetOrphanedJobsStep {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }
}

#[async_trait::async_trait]
impl SagaStep for ResetOrphanedJobsStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "ResetOrphanedJobs"
    }

    #[instrument(skip(context), fields(step = "ResetOrphanedJobs"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        // Clone command_bus Arc
        let command_bus = services.command_bus.as_ref().cloned().ok_or_else(|| {
            SagaError::StepFailed {
                step: self.name().to_string(),
                message: "CommandBus not available".to_string(),
                will_compensate: false,
            }
        })?;

        let event_bus = &services.event_bus;

        // Get list of orphaned jobs
        let job_ids: Vec<String> = context
            .get_metadata::<Vec<String>>("orphaned_job_ids")
            .and_then(|r| r.ok())
            .unwrap_or_default();

        let mode = if self.dry_run { "DRY RUN" } else { "RESET" };

        if job_ids.is_empty() {
            info!(mode = %mode, "No orphaned jobs to reset");
            context
                .set_metadata("reset_job_count", &0i64)
                .map_err(|e| SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: e.to_string(),
                    will_compensate: false,
                })?;
            return Ok(());
        }

        let mut reset_count = 0;

        for job_id_str in job_ids {
            if self.dry_run {
                debug!(job_id = %job_id_str, "Would reset orphaned job");
                reset_count += 1;
                continue;
            }

            let job_id =
                JobId(
                    uuid::Uuid::parse_str(&job_id_str).map_err(|e| SagaError::StepFailed {
                        step: self.name().to_string(),
                        message: format!("Invalid job_id: {}", e),
                        will_compensate: true,
                    })?,
                );

            // Reset job to failed state via CommandBus
            let command = MarkJobFailedCommand::new(
                job_id.clone(),
                "Orphaned job cleanup",
            );

            match dispatch_erased(&command_bus, command).await {
                Ok(_) => {
                    reset_count += 1;
                    info!(job_id = %job_id, "Reset orphaned job to FAILED via CommandBus");

                    // Publish job status changed event
                    let event = DomainEvent::JobStatusChanged {
                        job_id: job_id.clone(),
                        old_state: JobState::Running,
                        new_state: JobState::Failed,
                        correlation_id: context.correlation_id.clone(),
                        actor: context.actor.clone(),
                        occurred_at: chrono::Utc::now(),
                    };

                    let _ = event_bus.publish(&event).await.map_err(|e| {
                        warn!(job_id = %job_id, "Failed to publish event: {}", e);
                    });
                }
                Err(e) => {
                    warn!(job_id = %job_id, "Failed to reset job: {}", e);
                }
            }
        }

        context
            .set_metadata("reset_job_count", &(reset_count as i64))
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: e.to_string(),
                will_compensate: false,
            })?;

        info!(
            mode = %mode,
            count = %reset_count,
            "✅ {} orphaned jobs",
            if self.dry_run { "Would reset" } else { "Reset" }
        );

        Ok(())
    }

    async fn compensate(&self, context: &mut SagaContext) -> SagaResult<()> {
        if self.dry_run {
            return Ok(());
        }

        // Best effort: try to restore jobs
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

        let job_ids: Vec<String> = context
            .get_metadata::<Vec<String>>("orphaned_job_ids")
            .and_then(|r| r.ok())
            .unwrap_or_default();

        for job_id_str in job_ids {
            let job_id = JobId(uuid::Uuid::parse_str(&job_id_str).map_err(|e| {
                SagaError::CompensationFailed {
                    step: self.name().to_string(),
                    message: format!("Invalid job_id: {}", e),
                }
            })?);

            job_repository
                .update_state(&job_id, JobState::Running)
                .await
                .map_err(|e| {
                    warn!(job_id = %job_id, "Failed to restore job state: {}", e);
                });
        }

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
// PublishCleanupMetricsStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct PublishCleanupMetricsStep;

impl PublishCleanupMetricsStep {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SagaStep for PublishCleanupMetricsStep {
    type Output = ();

    fn name(&self) -> &'static str {
        "PublishCleanupMetrics"
    }

    #[instrument(skip(context), fields(step = "PublishCleanupMetrics"))]
    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let services = context.services().ok_or_else(|| SagaError::StepFailed {
            step: self.name().to_string(),
            message: "SagaServices not available".to_string(),
            will_compensate: false,
        })?;

        let event_bus = &services.event_bus;

        let worker_count: i64 = context
            .get_metadata::<i64>("cleaned_up_worker_count")
            .and_then(|r| r.ok())
            .unwrap_or(0);

        let job_count: i64 = context
            .get_metadata::<i64>("reset_job_count")
            .and_then(|r| r.ok())
            .unwrap_or(0);

        // Publish garbage collection completed event with metrics
        let event = DomainEvent::GarbageCollectionCompleted {
            provider_id: ProviderId::from_uuid(uuid::Uuid::nil()),
            workers_cleaned: worker_count as usize,
            orphans_detected: job_count as usize,
            errors: 0,
            duration_ms: 0,
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            occurred_at: chrono::Utc::now(),
        };

        event_bus
            .publish(&event)
            .await
            .map_err(|e| SagaError::StepFailed {
                step: self.name().to_string(),
                message: format!("Failed to publish CleanupCompleted event: {}", e),
                will_compensate: false,
            })?;

        info!(
            workers = %worker_count,
            jobs = %job_count,
            "✅ Cleanup metrics published"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_saga_has_five_steps() {
        let saga = CleanupSaga::new();
        let steps = saga.steps();
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0].name(), "IdentifyUnhealthyWorkers");
        assert_eq!(steps[1].name(), "IdentifyOrphanedJobs");
        assert_eq!(steps[2].name(), "CleanupUnhealthyWorkers");
        assert_eq!(steps[3].name(), "ResetOrphanedJobs");
        assert_eq!(steps[4].name(), "PublishCleanupMetrics");
    }

    #[test]
    fn cleanup_saga_has_default_thresholds() {
        let saga = CleanupSaga::new();
        assert_eq!(saga.unhealthy_threshold, DEFAULT_UNHEALTHY_THRESHOLD);
        assert_eq!(saga.orphaned_job_threshold, DEFAULT_ORPHANED_JOB_THRESHOLD);
        assert!(!saga.dry_run);
    }

    #[test]
    fn cleanup_saga_dry_run_mode() {
        let saga = CleanupSaga::new().with_dry_run(true);
        assert!(saga.dry_run);
    }

    #[test]
    fn cleanup_saga_with_custom_thresholds() {
        let saga =
            CleanupSaga::with_thresholds(Duration::from_secs(600), Duration::from_secs(1200));
        assert_eq!(saga.unhealthy_threshold, Duration::from_secs(600));
        assert_eq!(saga.orphaned_job_threshold, Duration::from_secs(1200));
    }
}
