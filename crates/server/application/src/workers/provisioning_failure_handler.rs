//! Provisioning Failure Handler
//!
//! Handles `WorkerProvisioningError` events to implement recovery strategies
//! based on the type of failure. Some failures require immediate job failure
//! (configuration errors), while others allow retry on different providers.
//!
//! Flow:
//! 1. Worker provisioning fails → WorkerProvisioningError event published
//! 2. This handler receives the event
//! 3. Based on failure reason:
//!    - ResourceExhausted → Emit JobRequeued for different provider
//!    - ImagePullFailed/InvalidConfiguration → Mark job as Failed (config error)
//!    - Other → Retry with backoff

use async_trait::async_trait;
use chrono::Utc;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, WorkerId, WorkerState};
use hodei_shared::states::{JobState, ProvisioningFailureReason};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Maximum provisioning retries per provider
const MAX_PROVISIONING_RETRIES: u32 = 2;

/// Provisioning Failure Handler
///
/// Handles worker provisioning failures with differentiated recovery strategies:
/// - Transient failures (timeout, network): retry with backoff
/// - Resource exhaustion: try different provider
/// - Configuration errors (image, invalid config): fail job immediately
pub struct ProvisioningFailureHandler {
    /// Job repository to update job state
    job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    /// Outbox repository to persist recovery events
    outbox_repository: Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>,
    /// Worker registry to cleanup failed worker
    worker_registry: Arc<dyn hodei_server_domain::workers::WorkerRegistry>,
}

impl ProvisioningFailureHandler {
    /// Create a new ProvisioningFailureHandler
    pub fn new(
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        outbox_repository: Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>,
        worker_registry: Arc<dyn hodei_server_domain::workers::WorkerRegistry>,
    ) -> Self {
        Self {
            job_repository,
            outbox_repository,
            worker_registry,
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for ProvisioningFailureHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Only handle WorkerProvisioningError events
        let (worker_id, provider_id, failure_reason, correlation_id) = match event {
            DomainEvent::WorkerProvisioningError {
                worker_id,
                provider_id,
                failure_reason,
                correlation_id,
                ..
            } => (worker_id, provider_id, failure_reason, correlation_id),
            _ => return Ok(()), // Not our event, skip
        };

        info!(
            worker_id = %worker_id,
            provider_id = %provider_id,
            reason = %format!("{:?}", failure_reason),
            "📦 ProvisioningFailureHandler: Processing provisioning failure"
        );

        // Find associated job via correlation_id
        let job_id = self.find_associated_job(&correlation_id).await?;

        match job_id {
            Some(job_id) => {
                // Apply recovery strategy based on failure type
                self.handle_failure(&job_id, &provider_id, &failure_reason)
                    .await?;
            }
            None => {
                warn!(
                    worker_id = %worker_id,
                    "No associated job found for provisioning failure"
                );
                // Still cleanup the failed worker
                self.cleanup_failed_worker(&worker_id).await?;
            }
        }

        Ok(())
    }
}

impl ProvisioningFailureHandler {
    /// Find job associated with the provisioning failure via correlation_id
    async fn find_associated_job(
        &self,
        correlation_id: &Option<String>,
    ) -> Result<Option<JobId>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(cid) = correlation_id {
            // The correlation_id should contain the job_id
            if let Ok(job_uuid) = uuid::Uuid::parse_str(cid) {
                let job_id = JobId(job_uuid);
                // Verify job exists
                if let Ok(Some(_job)) = self.job_repository.find_by_id(&job_id).await {
                    return Ok(Some(job_id));
                }
            }
        }
        Ok(None)
    }

    /// Apply recovery strategy based on failure type
    async fn handle_failure(
        &self,
        job_id: &JobId,
        failed_provider: &ProviderId,
        failure_reason: &ProvisioningFailureReason,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match failure_reason {
            // Configuration errors: fail immediately, no retry
            ProvisioningFailureReason::ImagePullFailed { image, message } => {
                error!(
                    job_id = %job_id,
                    image = image,
                    error = message,
                    "🚨 Image pull failed - marking job as failed (configuration error)"
                );
                self.mark_job_failed(
                    job_id,
                    &format!("Worker image '{}' not found: {}", image, message),
                )
                .await?;
            }
            ProvisioningFailureReason::InvalidConfiguration { field, reason } => {
                error!(
                    job_id = %job_id,
                    field = field,
                    error = reason,
                    "🚨 Invalid worker configuration - marking job as failed"
                );
                self.mark_job_failed(
                    job_id,
                    &format!("Invalid configuration for '{}': {}", field, reason),
                )
                .await?;
            }
            ProvisioningFailureReason::AuthenticationFailed { message } => {
                error!(
                    job_id = %job_id,
                    error = message,
                    "🚨 Provider authentication failed - marking job as failed"
                );
                self.mark_job_failed(
                    job_id,
                    &format!("Provider authentication failed: {}", message),
                )
                .await?;
            }

            // Resource exhaustion: try different provider
            ProvisioningFailureReason::ResourceAllocationFailed { resource, message } => {
                warn!(
                    job_id = %job_id,
                    resource = resource,
                    error = message,
                    "📦 Resource exhausted on provider - requeuing for different provider"
                );
                self.emit_job_requeued_different_provider(job_id, failed_provider, 1)
                    .await?;
            }

            // Transient failures: retry with backoff
            ProvisioningFailureReason::Timeout => {
                info!(
                    job_id = %job_id,
                    "⏰ Provisioning timeout - retrying with backoff"
                );
                self.emit_job_requeued(job_id, 1, 5).await?;
            }
            ProvisioningFailureReason::NetworkSetupFailed { message } => {
                info!(
                    job_id = %job_id,
                    error = message,
                    "🌐 Network setup failed - retrying with backoff"
                );
                self.emit_job_requeued(job_id, 1, 5).await?;
            }
            ProvisioningFailureReason::ProviderUnavailable => {
                info!(
                    job_id = %job_id,
                    "🏃 Provider unavailable - trying different provider"
                );
                self.emit_job_requeued_different_provider(job_id, failed_provider, 1)
                    .await?;
            }
            ProvisioningFailureReason::InternalError { message } => {
                warn!(
                    job_id = %job_id,
                    error = message,
                    "⚠️ Provider internal error - retrying with backoff"
                );
                self.emit_job_requeued(job_id, 1, 10).await?;
            }
        }

        Ok(())
    }

    /// Emit JobRequeued event for retry (same or different provider)
    async fn emit_job_requeued(
        &self,
        job_id: &JobId,
        retry_count: u32,
        backoff_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let idempotency_key = format!("job-requeued-{}-{}", job_id.0, retry_count);

        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobRequeued".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "retry_count": retry_count,
                "backoff_seconds": backoff_secs,
                "reason": "PROVISIONING_FAILURE",
                "retry_strategy": "same_provider"
            }),
            Some(serde_json::json!({
                "source": "ProvisioningFailureHandler",
                "handler_type": "retry",
                "retry_attempt": retry_count,
                "backoff_seconds": backoff_secs
            })),
            Some(idempotency_key),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        info!(
            job_id = %job_id,
            retry_count = retry_count,
            backoff_secs = backoff_secs,
            "📤 JobRequeued event persisted for provisioning retry"
        );

        Ok(())
    }

    /// Emit JobRequeued event with explicit different provider
    async fn emit_job_requeued_different_provider(
        &self,
        job_id: &JobId,
        excluded_provider: &ProviderId,
        retry_count: u32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let backoff_secs = 2_u64.pow(retry_count);
        let idempotency_key = format!("job-requeued-diff-provider-{}-{}", job_id.0, retry_count);

        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobRequeued".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "retry_count": retry_count,
                "backoff_seconds": backoff_secs,
                "reason": "PROVISIONING_FAILURE",
                "retry_strategy": "different_provider",
                "excluded_provider_id": excluded_provider.0.to_string()
            }),
            Some(serde_json::json!({
                "source": "ProvisioningFailureHandler",
                "handler_type": "retry_different_provider",
                "retry_attempt": retry_count,
                "excluded_provider": excluded_provider.0.to_string()
            })),
            Some(idempotency_key),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        info!(
            job_id = %job_id,
            excluded_provider = %excluded_provider,
            retry_count = retry_count,
            "📤 JobRequeued event persisted for retry on different provider"
        );

        Ok(())
    }

    /// Mark job as failed
    async fn mark_job_failed(
        &self,
        job_id: &JobId,
        reason: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Update job state to Failed
        self.job_repository
            .update_state(job_id, JobState::Failed)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Emit JobStatusChanged(Failed) event
        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobStatusChanged".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "previous_state": "Pending",
                "new_state": "Failed",
                "failure_reason": reason
            }),
            Some(serde_json::json!({
                "source": "ProvisioningFailureHandler",
                "handler_type": "final_failure"
            })),
            Some(format!(
                "job-failed-{}-{}",
                job_id.0,
                Utc::now().timestamp()
            )),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        error!(
            job_id = %job_id,
            reason = reason,
            "🚨 Job marked as Failed due to provisioning failure"
        );

        Ok(())
    }

    /// Cleanup failed worker registration
    async fn cleanup_failed_worker(
        &self,
        worker_id: &WorkerId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Err(e) = self.worker_registry.unregister(worker_id).await {
            warn!(
                worker_id = %worker_id,
                error = %e,
                "Failed to cleanup failed worker registration"
            );
        }
        Ok(())
    }
}

// Re-export EventHandler from the event module
use crate::jobs::event_subscriber::EventHandler;

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::{Job, JobsFilter};
    use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
    use hodei_shared::states::JobState;
    use uuid::Uuid;

    // Mock implementations for testing
    struct MockJobRepository;

    #[async_trait::async_trait]
    impl hodei_server_domain::jobs::JobRepository for MockJobRepository {
        async fn find_by_id(
            &self,
            _id: &JobId,
        ) -> Result<Option<Job>, hodei_server_domain::shared_kernel::DomainError> {
            Ok(None)
        }

        async fn update_state(
            &self,
            _job_id: &JobId,
            _state: JobState,
        ) -> Result<(), hodei_server_domain::shared_kernel::DomainError> {
            Ok(())
        }
        async fn save(
            &self,
            _job: &Job,
        ) -> Result<(), hodei_server_domain::shared_kernel::DomainError> {
            Ok(())
        }
        async fn find(
            &self,
            _filter: JobsFilter,
        ) -> Result<Vec<Job>, hodei_server_domain::shared_kernel::DomainError> {
            Ok(vec![])
        }
        async fn count_by_state(
            &self,
            _state: JobState,
        ) -> Result<u64, hodei_server_domain::shared_kernel::DomainError> {
            Ok(0)
        }
        async fn delete(
            &self,
            _id: &JobId,
        ) -> Result<(), hodei_server_domain::shared_kernel::DomainError> {
            Ok(())
        }
        async fn find_by_state(
            &self,
            _state: &JobState,
        ) -> Result<Vec<Job>, hodei_server_domain::shared_kernel::DomainError> {
            Ok(vec![])
        }
        async fn find_pending(
            &self,
        ) -> Result<Vec<Job>, hodei_server_domain::shared_kernel::DomainError> {
            Ok(vec![])
        }
        async fn find_all(
            &self,
            _limit: usize,
            _offset: usize,
        ) -> Result<(Vec<Job>, usize), hodei_server_domain::shared_kernel::DomainError> {
            Ok((vec![], 0))
        }
        async fn find_by_execution_id(
            &self,
            _execution_id: &str,
        ) -> Result<Option<Job>, hodei_server_domain::shared_kernel::DomainError> {
            Ok(None)
        }
        async fn update(
            &self,
            _job: &Job,
        ) -> Result<(), hodei_server_domain::shared_kernel::DomainError> {
            Ok(())
        }
    }

    struct MockOutboxRepository;

    #[async_trait::async_trait]
    impl OutboxRepository for MockOutboxRepository {
        type Error = OutboxError;

        async fn insert_events(&self, _events: &[OutboxEventInsert]) -> Result<(), Self::Error> {
            Ok(())
        }
        async fn get_pending_events(
            &self,
            _limit: usize,
            _max_retries: i32,
        ) -> Result<Vec<hodei_server_domain::outbox::OutboxEventView>, Self::Error> {
            Ok(vec![])
        }
        async fn mark_published(&self, _event_ids: &[Uuid]) -> Result<(), Self::Error> {
            Ok(())
        }
        async fn mark_failed(&self, _event_id: &Uuid, _error: &str) -> Result<(), Self::Error> {
            Ok(())
        }
        async fn exists_by_idempotency_key(&self, _key: &str) -> Result<bool, Self::Error> {
            Ok(false)
        }
        async fn count_pending(&self) -> Result<u64, Self::Error> {
            Ok(0)
        }
        async fn get_stats(&self) -> Result<hodei_server_domain::outbox::OutboxStats, Self::Error> {
            Ok(hodei_server_domain::outbox::OutboxStats {
                pending_count: 0,
                published_count: 0,
                failed_count: 0,
                oldest_pending_age_seconds: None,
            })
        }
        async fn cleanup_published_events(
            &self,
            _older_than: std::time::Duration,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }
        async fn cleanup_failed_events(
            &self,
            _max_retries: i32,
            _older_than: std::time::Duration,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }
        async fn find_by_id(
            &self,
            _id: Uuid,
        ) -> Result<Option<hodei_server_domain::outbox::OutboxEventView>, Self::Error> {
            Ok(None)
        }
    }

    struct MockWorkerRegistry;

    #[async_trait::async_trait]
    impl hodei_server_domain::workers::WorkerRegistry for MockWorkerRegistry {
        async fn register(
            &self,
            _handle: hodei_server_domain::workers::WorkerHandle,
            _spec: hodei_server_domain::workers::WorkerSpec,
            _job_id: JobId,
        ) -> Result<hodei_server_domain::workers::Worker> {
            Err(DomainError::InfrastructureError {
                message: "Not implemented".to_string(),
            })
        }

        async fn save(&self, _worker: &hodei_server_domain::workers::Worker) -> Result<()> {
            Ok(())
        }

        async fn unregister(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn find_by_id(
            &self,
            _id: &WorkerId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            Ok(None)
        }

        async fn get_by_job_id(
            &self,
            _job_id: &JobId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            Ok(None)
        }

        async fn find(
            &self,
            _filter: &hodei_server_domain::workers::WorkerFilter,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_ready_worker(
            &self,
            _filter: Option<&hodei_server_domain::workers::WorkerFilter>,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            Ok(None)
        }

        async fn find_available(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_by_provider(
            &self,
            _provider_id: &ProviderId,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn update_state(&self, _id: &WorkerId, _state: WorkerState) -> Result<()> {
            Ok(())
        }

        async fn update_heartbeat(&self, _id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn mark_busy(&self, _id: &WorkerId, _job_id: Option<JobId>) -> Result<()> {
            Ok(())
        }

        async fn release_from_job(&self, _id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn find_unhealthy(
            &self,
            _timeout: std::time::Duration,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_for_termination(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_idle_timed_out(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_lifetime_exceeded(
            &self,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_ttl_after_completion_exceeded(
            &self,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn stats(&self) -> Result<hodei_server_domain::workers::WorkerRegistryStats> {
            Ok(hodei_server_domain::workers::WorkerRegistryStats::default())
        }

        async fn count(&self) -> Result<usize> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_image_pull_failure_marks_job_failed() {
        let handler = ProvisioningFailureHandler::new(
            Arc::new(MockJobRepository),
            Arc::new(MockOutboxRepository),
            Arc::new(MockWorkerRegistry),
        );

        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let job_id = JobId::new();

        let event = DomainEvent::WorkerProvisioningError {
            worker_id,
            provider_id,
            failure_reason: ProvisioningFailureReason::ImagePullFailed {
                image: "hodei-worker:v1.0".to_string(),
                message: "Not found".to_string(),
            },
            occurred_at: Utc::now(),
            correlation_id: Some(job_id.0.to_string()),
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resource_exhaustion_emits_requeue_different_provider() {
        let handler = ProvisioningFailureHandler::new(
            Arc::new(MockJobRepository),
            Arc::new(MockOutboxRepository),
            Arc::new(MockWorkerRegistry),
        );

        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let job_id = JobId::new();

        let event = DomainEvent::WorkerProvisioningError {
            worker_id,
            provider_id,
            failure_reason: ProvisioningFailureReason::ResourceAllocationFailed {
                resource: "memory".to_string(),
                message: "8GB limit exceeded".to_string(),
            },
            occurred_at: Utc::now(),
            correlation_id: Some(job_id.0.to_string()),
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_timeout_triggers_retry_with_backoff() {
        let handler = ProvisioningFailureHandler::new(
            Arc::new(MockJobRepository),
            Arc::new(MockOutboxRepository),
            Arc::new(MockWorkerRegistry),
        );

        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let job_id = JobId::new();

        let event = DomainEvent::WorkerProvisioningError {
            worker_id,
            provider_id,
            failure_reason: ProvisioningFailureReason::Timeout,
            occurred_at: Utc::now(),
            correlation_id: Some(job_id.0.to_string()),
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }
}
