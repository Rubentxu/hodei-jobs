//!
//! # Event Handlers for saga-engine v4.0 (EPIC-94-C.5)
//!
//! This module provides event handlers for domain events that integrate
//! with the new DurableWorkflow-based system.
//!
//! ## Key Handlers
//!
//! - `WorkerReadyEventHandler`: Dispatch jobs reactively when workers are ready
//! - `JobQueuedEventHandler`: Process newly queued jobs
//! - `SagaEventHandler`: Handle saga completion/failure events
//!

use anyhow::Context as AnyhowContext;
use async_trait::async_trait;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::outbox::{OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::JobId;
use hodei_server_domain::workers::{WorkerFilter, WorkerRegistry};
use hodei_shared::WorkerId;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use super::event_subscriber::EventHandler;
use crate::saga::provisioning_workflow_coordinator::ProvisioningWorkflowCoordinator;
use crate::scheduling::SchedulerConfig;
use hodei_server_domain::jobs::{JobRepository, JobSpec};
use hodei_server_domain::providers::ProviderConfigRepository;
use hodei_server_domain::scheduling::ProviderInfo;
use hodei_server_domain::shared_kernel::ProviderId;

/// Handler for WorkerReadyForJob events
///
/// When a worker becomes ready, this handler finds a matching job and dispatches it.
/// This enables reactive job dispatch without polling.
pub struct WorkerReadyEventHandler {
    /// Job repository to fetch pending jobs
    job_repository: Arc<dyn JobRepository>,
    /// Worker registry for finding the ready worker
    worker_registry: Arc<dyn WorkerRegistry>,
    /// Provider registry for selecting best job
    provider_registry: Arc<dyn ProviderConfigRepository>,
    /// Scheduler configuration for job selection
    scheduler_config: SchedulerConfig,
    /// Outbox for publishing events
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
}

impl WorkerReadyEventHandler {
    /// Create a new handler
    pub fn new(
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        provider_registry: Arc<dyn ProviderConfigRepository>,
        scheduler_config: SchedulerConfig,
        outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
            provider_registry,
            scheduler_config,
            outbox_repository,
        }
    }

    /// Find a pending job for the given worker
    async fn find_job_for_worker(&self, worker_id: &WorkerId) -> Option<JobId> {
        // Get worker details
        let worker = self.worker_registry.get(worker_id).await.ok().flatten()?;

        // Check if worker has an associated job
        worker.current_job_id().cloned()
    }

    /// Emit JobAssigned event for audit trail
    async fn emit_job_assigned_event(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> anyhow::Result<()> {
        if let Some(ref outbox_repo) = self.outbox_repository {
            let event = OutboxEventInsert::for_job(
                job_id.0,
                "JobAssigned".to_string(),
                serde_json::json!({
                    "job_id": job_id.0.to_string(),
                    "worker_id": worker_id.0.to_string(),
                    "source": "WorkerReadyEventHandler"
                }),
                None,
                None,
            );

            outbox_repo
                .insert_events(&[event])
                .await
                .map_err(|e| anyhow::anyhow!("Failed to emit JobAssigned event: {}", e))?;

            debug!(job_id = %job_id, worker_id = %worker_id, "📤 JobAssigned event emitted");
        }
        Ok(())
    }
}

#[async_trait]
impl EventHandler for WorkerReadyEventHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> anyhow::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Only handle WorkerReadyForJob events
        let (worker_id, provider_id, correlation_id, _actor) = match event {
            DomainEvent::WorkerReadyForJob {
                worker_id,
                provider_id,
                correlation_id,
                actor,
                ..
            } => (worker_id, provider_id, correlation_id, actor),
            _ => return Ok(()), // Not our event
        };

        info!(
            worker_id = %worker_id,
            provider_id = %provider_id,
            correlation_id = ?correlation_id,
            "🔔 WorkerReadyEventHandler: Worker {} ready on provider {}",
            worker_id, provider_id
        );

        // Find a job for this worker
        if let Some(job_id) = self.find_job_for_worker(&worker_id).await {
            info!(
                worker_id = %worker_id,
                job_id = %job_id,
                "Found job {} for worker {}, emitting JobAssigned",
                job_id, worker_id
            );

            // Emit JobAssigned event (actual dispatch happens through gRPC)
            self.emit_job_assigned_event(&job_id, &worker_id).await?;
        } else {
            debug!(
                worker_id = %worker_id,
                "No job found for worker (may be waiting for job to be queued)"
            );
        }

        Ok(())
    }
}

/// Handler for JobQueued events
///
/// When a job is queued, this handler checks for available workers
/// and either dispatches immediately or requests worker provisioning.
pub struct JobQueuedEventHandler {
    /// Job repository
    job_repository: Arc<dyn JobRepository>,
    /// Worker registry
    worker_registry: Arc<dyn WorkerRegistry>,
    /// Provider registry
    provider_registry: Arc<dyn ProviderConfigRepository>,
    /// Scheduler config
    scheduler_config: SchedulerConfig,
    /// Workflow coordinator for provisioning
    provisioning_coordinator: Option<Arc<dyn ProvisioningWorkflowCoordinator>>,
    /// Outbox for events
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
}

impl JobQueuedEventHandler {
    /// Create a new handler
    pub fn new(
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        provider_registry: Arc<dyn ProviderConfigRepository>,
        scheduler_config: SchedulerConfig,
        provisioning_coordinator: Option<Arc<dyn ProvisioningWorkflowCoordinator>>,
        outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
            provider_registry,
            scheduler_config,
            provisioning_coordinator,
            outbox_repository,
        }
    }
}

#[async_trait]
impl EventHandler for JobQueuedEventHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> anyhow::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Only handle JobQueued events
        let (job_id, preferred_provider, correlation_id, _actor) = match event {
            DomainEvent::JobQueued {
                job_id,
                preferred_provider,
                correlation_id,
                actor,
                ..
            } => (job_id, preferred_provider, correlation_id, actor),
            _ => return Ok(()),
        };

        info!(
            job_id = %job_id,
            preferred_provider = ?preferred_provider,
            correlation_id = ?correlation_id,
            "🔔 JobQueuedEventHandler: Processing new job"
        );

        // Fetch the job
        let job = match self.job_repository.find_by_id(&job_id).await? {
            Some(job) => job,
            None => {
                warn!(job_id = %job_id, "Job not found in repository");
                return Ok(());
            }
        };

        // Check for available workers
        let available_workers = self.worker_registry.find_available().await?;

        if !available_workers.is_empty() {
            info!(
                job_id = %job_id,
                workers_count = available_workers.len(),
                "Found {} available workers, job ready for dispatch",
                available_workers.len()
            );
            // The dispatcher will pick up this job on its next cycle
            // or we could emit WorkerProvisioningRequested if no worker matches
        } else {
            info!(
                job_id = %job_id,
                "No workers available, emitting WorkerProvisioningRequested"
            );

            // Select provider
            let provider_id = if let Some(p) = preferred_provider {
                p
            } else {
                // Get provider info and select best
                let providers = self.provider_registry.find_all().await?;
                if let Some(config) = providers.first() {
                    config.id.clone()
                } else {
                    warn!(job_id = %job_id, "No providers available");
                    return Ok(());
                }
            };

            // Emit WorkerProvisioningRequested event
            self.emit_provisioning_requested(&job_id, &provider_id)
                .await?;
        }

        Ok(())
    }
}

impl JobQueuedEventHandler {
    /// Emit WorkerProvisioningRequested event
    async fn emit_provisioning_requested(
        &self,
        job_id: &JobId,
        provider_id: &ProviderId,
    ) -> anyhow::Result<()> {
        if let Some(ref outbox_repo) = self.outbox_repository {
            let event = OutboxEventInsert::for_job(
                job_id.0,
                "WorkerProvisioningRequested".to_string(),
                serde_json::json!({
                    "job_id": job_id.0.to_string(),
                    "provider_id": provider_id.0.to_string(),
                    "source": "JobQueuedEventHandler"
                }),
                None,
                None,
            );

            outbox_repo
                .insert_events(&[event])
                .await
                .map_err(|e| anyhow::anyhow!("Failed to emit event: {}", e))?;

            debug!(job_id = %job_id, provider_id = %provider_id, "📤 WorkerProvisioningRequested emitted");
        }
        Ok(())
    }
}

/// Handler for saga completion/failure events
///
/// Provides audit trail and metrics for saga-based workflows.
pub struct SagaEventHandler {
    /// Outbox for persisting saga events
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
}

impl SagaEventHandler {
    /// Create a new handler
    pub fn new(outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>) -> Self {
        Self { outbox_repository }
    }
}

#[async_trait]
impl EventHandler for SagaEventHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> anyhow::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match event {
            DomainEvent::SagaCompleted {
                saga_id,
                saga_type,
                duration_ms,
                steps_executed,
                correlation_id,
                occurred_at,
            } => {
                info!(
                    saga_id = %saga_id,
                    saga_type = %saga_type,
                    duration_ms = %duration_ms,
                    steps_executed = %steps_executed,
                    correlation_id = ?correlation_id,
                    "✅ SagaCompleted: {} ({})",
                    saga_type, saga_id
                );
            }
            DomainEvent::SagaFailed {
                saga_id,
                saga_type,
                error_message,
                failed_at_step,
                compensation_triggered,
                correlation_id,
                occurred_at,
            } => {
                error!(
                    saga_id = %saga_id,
                    saga_type = %saga_type,
                    error = %error_message,
                    failed_at_step = %failed_at_step,
                    compensation_triggered = %compensation_triggered,
                    correlation_id = ?correlation_id,
                    "❌ SagaFailed: {} ({}) at step {}",
                    saga_type, saga_id, failed_at_step
                );
            }
            DomainEvent::SagaTimedOut {
                saga_id,
                saga_type,
                timeout_duration_ms,
                elapsed_ms,
                steps_completed,
                correlation_id,
                occurred_at,
            } => {
                warn!(
                    saga_id = %saga_id,
                    saga_type = %saga_type,
                    timeout_ms = %timeout_duration_ms,
                    elapsed_ms = %elapsed_ms,
                    steps_completed = %steps_completed,
                    correlation_id = ?correlation_id,
                    "⏱️ SagaTimedOut: {} ({})",
                    saga_type, saga_id
                );
            }
            _ => return Ok(()),
        }

        Ok(())
    }
}

/// Builder for creating event handlers with common configuration
pub struct EventHandlerBuilder {
    job_repository: Option<Arc<dyn JobRepository>>,
    worker_registry: Option<Arc<dyn WorkerRegistry>>,
    provider_registry: Option<Arc<dyn ProviderConfigRepository>>,
    scheduler_config: Option<SchedulerConfig>,
    provisioning_coordinator: Option<Arc<dyn ProvisioningWorkflowCoordinator>>,
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
}

impl EventHandlerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            job_repository: None,
            worker_registry: None,
            provider_registry: None,
            scheduler_config: None,
            provisioning_coordinator: None,
            outbox_repository: None,
        }
    }

    /// Set job repository
    pub fn with_job_repository(mut self, job_repository: Arc<dyn JobRepository>) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    /// Set worker registry
    pub fn with_worker_registry(mut self, worker_registry: Arc<dyn WorkerRegistry>) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    /// Set provider registry
    pub fn with_provider_registry(
        mut self,
        provider_registry: Arc<dyn ProviderConfigRepository>,
    ) -> Self {
        self.provider_registry = Some(provider_registry);
        self
    }

    /// Set scheduler config
    pub fn with_scheduler_config(mut self, scheduler_config: SchedulerConfig) -> Self {
        self.scheduler_config = Some(scheduler_config);
        self
    }

    /// Set provisioning coordinator
    pub fn with_provisioning_coordinator(
        mut self,
        coordinator: Arc<dyn ProvisioningWorkflowCoordinator>,
    ) -> Self {
        self.provisioning_coordinator = Some(coordinator);
        self
    }

    /// Set outbox repository
    pub fn with_outbox_repository(
        mut self,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    ) -> Self {
        self.outbox_repository = Some(outbox_repository);
        self
    }

    /// Build WorkerReadyEventHandler
    pub fn build_worker_ready_handler(
        self,
    ) -> Result<WorkerReadyEventHandler, Box<dyn std::error::Error + Send + Sync>> {
        Ok(WorkerReadyEventHandler::new(
            self.job_repository.ok_or("job_repository required")?,
            self.worker_registry.ok_or("worker_registry required")?,
            self.provider_registry.ok_or("provider_registry required")?,
            self.scheduler_config.unwrap_or_default(),
            self.outbox_repository,
        ))
    }

    /// Build JobQueuedEventHandler
    pub fn build_job_queued_handler(
        self,
    ) -> Result<JobQueuedEventHandler, Box<dyn std::error::Error + Send + Sync>> {
        Ok(JobQueuedEventHandler::new(
            self.job_repository.ok_or("job_repository required")?,
            self.worker_registry.ok_or("worker_registry required")?,
            self.provider_registry.ok_or("provider_registry required")?,
            self.scheduler_config.unwrap_or_default(),
            self.provisioning_coordinator,
            self.outbox_repository,
        ))
    }

    /// Build SagaEventHandler
    pub fn build_saga_handler(self) -> SagaEventHandler {
        SagaEventHandler::new(self.outbox_repository)
    }

    /// Build JobAssignedEventHandler
    pub fn build_job_assigned_handler(
        self,
    ) -> Result<JobAssignedEventHandler, Box<dyn std::error::Error + Send + Sync>> {
        Ok(JobAssignedEventHandler::new(
            self.job_repository.ok_or("job_repository required")?,
        ))
    }
}

impl Default for EventHandlerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for JobAssigned events
///
/// When a worker registers with a job_id, this handler updates the job.worker_id column
/// to maintain data consistency between the jobs and workers tables.
///
/// This fixes the data inconsistency where:
/// - workers.current_job_id is set during registration
/// - but jobs.worker_id was NOT being updated
///
/// The bidirectional relationship is now properly maintained.
pub struct JobAssignedEventHandler {
    /// Job repository to update worker_id
    job_repository: Arc<dyn JobRepository>,
}

impl JobAssignedEventHandler {
    /// Create a new handler
    pub fn new(job_repository: Arc<dyn JobRepository>) -> Self {
        Self { job_repository }
    }
}

#[async_trait::async_trait]
impl EventHandler for JobAssignedEventHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> anyhow::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Only handle JobAssigned events
        let (job_id, worker_id, correlation_id, actor) = match event {
            DomainEvent::JobAssigned {
                job_id,
                worker_id,
                occurred_at: _,
                correlation_id,
                actor,
            } => (job_id, worker_id, correlation_id, actor),
            _ => return Ok(()),
        };

        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            correlation_id = ?correlation_id,
            "🔔 JobAssignedEventHandler: Assigning worker {} to job {}",
            worker_id, job_id
        );

        // Update job.worker_id for data consistency
        // Note: We log errors but don't return them - following the pattern of other handlers
        // The handler processes events reactively, and logging is sufficient for observability
        if let Err(e) = self.job_repository.assign_worker(&job_id, &worker_id).await {
            error!(
                job_id = %job_id,
                worker_id = %worker_id,
                error = ?e,
                correlation_id = ?correlation_id,
                "❌ JobAssignedEventHandler: Failed to update job.worker_id - DB error: {}",
                e
            );
            // Don't return error - just log and continue processing
            return Ok(());
        }

        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            actor = ?actor,
            "✅ JobAssignedEventHandler: Worker {} successfully assigned to job {}",
            worker_id, job_id
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
    use hodei_server_domain::workers::{Worker, WorkerSpec};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_saga_event_handler_completed() {
        let handler = SagaEventHandler::new(None);

        let event = DomainEvent::SagaCompleted {
            saga_id: Uuid::new_v4(),
            saga_type: "Provisioning".to_string(),
            duration_ms: 1500,
            steps_executed: 3,
            correlation_id: Some("test-correlation".to_string()),
            occurred_at: chrono::Utc::now(),
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_saga_event_handler_failed() {
        let handler = SagaEventHandler::new(None);

        let event = DomainEvent::SagaFailed {
            saga_id: Uuid::new_v4(),
            saga_type: "Execution".to_string(),
            error_message: "Worker disconnected".to_string(),
            failed_at_step: 2,
            compensation_triggered: true,
            correlation_id: Some("test-correlation".to_string()),
            occurred_at: chrono::Utc::now(),
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_saga_event_handler_non_saga_event() {
        let handler = SagaEventHandler::new(None);

        let event = DomainEvent::JobQueued {
            job_id: JobId::new(),
            preferred_provider: None,
            job_requirements: JobSpec::new(vec![]),
            queued_at: chrono::Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }
}
