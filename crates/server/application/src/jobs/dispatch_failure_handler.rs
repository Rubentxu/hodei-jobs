//! Dispatch Failure Handler
//!
//! Handles `JobDispatchFailed` events to implement retry logic or mark jobs as failed
//! after maximum retries are exhausted. This prevents jobs from getting stuck in
//! `Assigned` state when dispatch fails.
//!
//! Flow:
//! 1. Dispatch fails → JobDispatchFailed event published
//! 2. This handler receives the event
//! 3. If retry_count < MAX_DISPATCH_RETRIES: emit JobRequeued for retry
//! 4. If max retries reached: mark job as Failed

use chrono::Utc;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::outbox::{OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::JobId;
use hodei_shared::states::{DispatchFailureReason, JobState};
use std::result::Result;
use std::sync::Arc;
use tracing::{error, info};

use super::event_subscriber::EventHandler;

/// Maximum number of dispatch retries before marking job as failed
const MAX_DISPATCH_RETRIES: u32 = 3;

/// Dispatch Failure Handler
///
/// Handles dispatch failures with configurable retry logic:
/// - Exponential backoff for retries
/// - Final failure state after max retries
/// - Event persistence for reliable processing
pub struct DispatchFailureHandler {
    /// Job repository to update job state
    job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    /// Outbox repository to persist requeue/failure events
    outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
}

impl DispatchFailureHandler {
    /// Create a new DispatchFailureHandler
    pub fn new(
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    ) -> Self {
        Self {
            job_repository,
            outbox_repository,
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for DispatchFailureHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Only handle JobDispatchFailed events
        let (job_id, worker_id, failure_reason, retry_count) = match event {
            DomainEvent::JobDispatchFailed {
                job_id,
                worker_id,
                failure_reason,
                retry_count,
                ..
            } => (job_id, worker_id, failure_reason, retry_count),
            _ => return Ok(()), // Not our event, skip
        };

        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            retry_count = retry_count,
            reason = %format!("{:?}", failure_reason),
            "📦 DispatchFailureHandler: Processing dispatch failure"
        );

        // Determine if we should retry or fail permanently
        if retry_count < MAX_DISPATCH_RETRIES {
            // Calculate exponential backoff
            let backoff_secs = 2_u64.pow(retry_count);
            self.emit_job_requeued(&job_id, retry_count + 1, backoff_secs)
                .await?;
        } else {
            // Max retries reached - mark job as failed
            self.mark_job_failed(&job_id, &failure_reason).await?;
        }

        Ok(())
    }
}

impl DispatchFailureHandler {
    /// Emit a JobRequeued event for retry with exponential backoff
    async fn emit_job_requeued(
        &self,
        job_id: &JobId,
        next_retry: u32,
        backoff_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let idempotency_key = format!("job-requeued-{}-{}", job_id.0, next_retry);
        let _now = Utc::now();

        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobRequeued".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "retry_count": next_retry,
                "backoff_seconds": backoff_secs,
                "reason": "DISPATCH_FAILURE"
            }),
            Some(serde_json::json!({
                "source": "DispatchFailureHandler",
                "handler_type": "retry",
                "retry_attempt": next_retry,
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
            retry_count = next_retry,
            backoff_secs = backoff_secs,
            "📤 JobRequeued event persisted for dispatch retry"
        );

        Ok(())
    }

    /// Mark job as failed after max retries exhausted
    async fn mark_job_failed(
        &self,
        job_id: &JobId,
        failure_reason: &DispatchFailureReason,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let reason_str = format!(
            "Dispatch failed after {} retries: {:?}",
            MAX_DISPATCH_RETRIES, failure_reason
        );

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
                "previous_state": "Assigned",
                "new_state": "Failed",
                "failure_reason": reason_str
            }),
            Some(serde_json::json!({
                "source": "DispatchFailureHandler",
                "handler_type": "final_failure",
                "max_retries": MAX_DISPATCH_RETRIES
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
            reason = %reason_str,
            "🚨 Job marked as Failed after {} dispatch retries",
            MAX_DISPATCH_RETRIES
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::JobCreated;
    use hodei_server_domain::jobs::{Job, JobsFilter};
    use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, WorkerId};
    use hodei_shared::states::{DispatchFailureReason, JobState};
    use uuid::Uuid;

    // Mock job repository for testing
    struct MockJobRepository {
        update_state_calls: std::sync::Arc<std::sync::Mutex<Vec<JobId>>>,
    }

    #[async_trait::async_trait]
    impl hodei_server_domain::jobs::JobRepository for MockJobRepository {
        async fn find_by_id(&self, _id: &JobId) -> std::result::Result<Option<Job>, DomainError> {
            Ok(None)
        }

        async fn update_state(
            &self,
            job_id: &JobId,
            _state: JobState,
        ) -> std::result::Result<(), DomainError> {
            let mut calls = self.update_state_calls.lock().unwrap();
            calls.push(job_id.clone());
            Ok(())
        }

        async fn save(&self, _job: &Job) -> std::result::Result<(), DomainError> {
            Ok(())
        }
        async fn find(&self, _filter: JobsFilter) -> std::result::Result<Vec<Job>, DomainError> {
            Ok(vec![])
        }
        async fn count_by_state(&self, _state: &JobState) -> std::result::Result<u64, DomainError> {
            Ok(0)
        }
        async fn delete(&self, _id: &JobId) -> std::result::Result<(), DomainError> {
            Ok(())
        }
        async fn find_by_state(
            &self,
            _state: &JobState,
        ) -> std::result::Result<Vec<Job>, DomainError> {
            Ok(vec![])
        }
        async fn find_pending(&self) -> std::result::Result<Vec<Job>, DomainError> {
            Ok(vec![])
        }
        async fn find_all(
            &self,
            _limit: usize,
            _offset: usize,
        ) -> std::result::Result<(Vec<Job>, usize), DomainError> {
            Ok((vec![], 0))
        }
        async fn find_by_execution_id(
            &self,
            _execution_id: &str,
        ) -> std::result::Result<Option<Job>, DomainError> {
            Ok(None)
        }
        async fn update(&self, _job: &Job) -> std::result::Result<(), DomainError> {
            Ok(())
        }
    }

    // Mock outbox repository for testing
    struct MockOutboxRepository;

    #[async_trait::async_trait]
    impl OutboxRepository for MockOutboxRepository {
        async fn insert_events(
            &self,
            _events: &[OutboxEventInsert],
        ) -> std::result::Result<(), OutboxError> {
            Ok(())
        }

        async fn get_pending_events(
            &self,
            _limit: usize,
            _max_retries: i32,
        ) -> std::result::Result<Vec<hodei_server_domain::outbox::OutboxEventView>, OutboxError>
        {
            Ok(vec![])
        }

        async fn mark_published(
            &self,
            _event_ids: &[Uuid],
        ) -> std::result::Result<(), OutboxError> {
            Ok(())
        }
        async fn mark_failed(
            &self,
            _event_id: &Uuid,
            _error: &str,
        ) -> std::result::Result<(), OutboxError> {
            Ok(())
        }
        async fn exists_by_idempotency_key(
            &self,
            _key: &str,
        ) -> std::result::Result<bool, OutboxError> {
            Ok(false)
        }
        async fn count_pending(&self) -> std::result::Result<u64, OutboxError> {
            Ok(0)
        }
        async fn get_stats(
            &self,
        ) -> std::result::Result<hodei_server_domain::outbox::OutboxStats, OutboxError> {
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
        ) -> std::result::Result<u64, OutboxError> {
            Ok(0)
        }
        async fn cleanup_failed_events(
            &self,
            _max_retries: i32,
            _older_than: std::time::Duration,
        ) -> std::result::Result<u64, OutboxError> {
            Ok(0)
        }
        async fn find_by_id(
            &self,
            _id: Uuid,
        ) -> std::result::Result<Option<hodei_server_domain::outbox::OutboxEventView>, OutboxError>
        {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_handler_ignores_non_dispatch_events() {
        let handler = DispatchFailureHandler::new(
            Arc::new(MockJobRepository {
                update_state_calls: Arc::new(std::sync::Mutex::new(vec![])),
            }),
            Arc::new(MockOutboxRepository),
        );

        // Create a different event type
        let job_id = JobId::new();
        let event = DomainEvent::JobCreated(JobCreated {
            job_id,
            spec: hodei_server_domain::jobs::JobSpec::new(vec!["echo".to_string()]),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        });

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_max_retries_triggers_failure() {
        let update_calls = Arc::new(std::sync::Mutex::new(vec![]));
        let handler = DispatchFailureHandler::new(
            Arc::new(MockJobRepository {
                update_state_calls: update_calls.clone(),
            }),
            Arc::new(MockOutboxRepository),
        );

        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();

        // Event with max retries reached
        let event = DomainEvent::JobDispatchFailed {
            job_id: job_id.clone(),
            worker_id,
            failure_reason: DispatchFailureReason::CommunicationTimeout { timeout_ms: 5000 },
            retry_count: MAX_DISPATCH_RETRIES, // Exactly max retries
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());

        // Verify job was marked as failed
        let calls = update_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], job_id);
    }
}
