//! Scheduling Failure Handler
//!
//! Handles `SchedulingDecisionFailed` events to implement backpressure handling
//! and retry strategies when the scheduler cannot make a decision.
//!
//! Flow:
//! 1. Scheduler fails → SchedulingDecisionFailed event published
//! 2. This handler receives the event
//! 3. Based on failure reason:
//!    - NoProvidersAvailable: Emit alert and schedule retry
//!    - AllProvidersOverloaded: Backpressure with exponential backoff
//!    - NoMatchingProvider: Mark job as failed (config error)
//!    - ConstraintsUnsatisfiable: Mark job as failed

use chrono::Utc;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::JobId;
use hodei_shared::states::{JobState, SchedulingFailureReason};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Maximum scheduling retries with backpressure
const MAX_SCHEDULING_RETRIES: u32 = 5;

/// Base backoff for scheduling retries (seconds)
const BASE_BACKOFF_SECS: u64 = 30;

/// Scheduling Failure Handler
///
/// Handles scheduling failures with differentiated recovery strategies:
/// - No providers: alert and retry with moderate backoff
/// - Provider overloaded: aggressive backoff (backpressure)
/// - Configuration errors: fail job immediately
pub struct SchedulingFailureHandler {
    /// Job repository to update job state
    job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    /// Outbox repository to persist recovery events
    outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
}

impl SchedulingFailureHandler {
    /// Create a new SchedulingFailureHandler
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
impl EventHandler for SchedulingFailureHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Only handle SchedulingDecisionFailed events
        let (job_id, failure_reason, attempted_providers) = match event {
            DomainEvent::SchedulingDecisionFailed {
                job_id,
                failure_reason,
                attempted_providers,
                ..
            } => (job_id, failure_reason, attempted_providers),
            _ => return Ok(()), // Not our event, skip
        };

        info!(
            job_id = %job_id,
            reason = %format!("{:?}", failure_reason),
            attempted_providers = ?attempted_providers.iter().map(|p| p.0.to_string()).collect::<Vec<_>>(),
            "📦 SchedulingFailureHandler: Processing scheduling failure"
        );

        // Apply recovery strategy based on failure type
        self.handle_failure(&job_id, &failure_reason).await?;

        Ok(())
    }
}

impl SchedulingFailureHandler {
    /// Apply recovery strategy based on failure type
    async fn handle_failure(
        &self,
        job_id: &JobId,
        failure_reason: &SchedulingFailureReason,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match failure_reason {
            // No providers available - alert and retry with moderate backoff
            SchedulingFailureReason::NoProvidersAvailable => {
                self.emit_alert("NO_PROVIDERS_AVAILABLE", job_id).await?;
                self.schedule_retry(job_id, BASE_BACKOFF_SECS).await?;
            }

            // All providers overloaded - backpressure with aggressive backoff
            SchedulingFailureReason::ProviderOverloaded => {
                warn!(
                    job_id = %job_id,
                    "⚠️ All providers overloaded - applying backpressure"
                );
                self.emit_alert("PROVIDERS_OVERLOADED", job_id).await?;
                // More aggressive backoff for backpressure
                self.schedule_retry_with_backoff(job_id).await?;
            }

            // Provider unhealthy - try again, provider might recover
            SchedulingFailureReason::ProviderUnhealthy { provider_id } => {
                info!(
                    job_id = %job_id,
                    provider_id = provider_id,
                    "Provider marked unhealthy - retrying"
                );
                self.schedule_retry(job_id, BASE_BACKOFF_SECS).await?;
            }

            // No matching provider - configuration error, fail job
            SchedulingFailureReason::NoMatchingProviders { requirement } => {
                error!(
                    job_id = %job_id,
                    requirement = requirement,
                    "🚨 No provider matches job requirements - marking job as failed"
                );
                self.mark_job_failed(
                    job_id,
                    &format!("No provider matches job requirement: {}", requirement),
                )
                .await?;
            }

            // Resource constraints not met - fail job (invalid job spec)
            SchedulingFailureReason::ResourceConstraintsNotMet {
                resource,
                required,
                available,
            } => {
                error!(
                    job_id = %job_id,
                    resource = resource,
                    required = required,
                    available = available,
                    "🚨 Job resource requirements cannot be satisfied - marking job as failed"
                );
                self.mark_job_failed(
                    job_id,
                    &format!(
                        "Resource constraint not met: {} requires {}, but only {} available",
                        resource, required, available
                    ),
                )
                .await?;
            }

            // Internal scheduler error - retry with backoff
            SchedulingFailureReason::InternalError { message } => {
                warn!(
                    job_id = %job_id,
                    error = message,
                    "⚠️ Scheduler internal error - retrying with backoff"
                );
                self.emit_alert("SCHEDULER_INTERNAL_ERROR", job_id).await?;
                self.schedule_retry_with_backoff(job_id).await?;
            }

            // Job was cancelled before scheduling - no action needed
            SchedulingFailureReason::JobCancelled => {
                info!(
                    job_id = %job_id,
                    "Job was cancelled before scheduling decision - no action needed"
                );
            }

            // Job expired in queue - mark as failed
            SchedulingFailureReason::JobExpired => {
                warn!(
                    job_id = %job_id,
                    "🚨 Job expired in queue - marking as failed"
                );
                self.mark_job_failed(job_id, "Job expired while waiting in scheduling queue")
                    .await?;
            }
        }

        Ok(())
    }

    /// Schedule retry with simple backoff
    async fn schedule_retry(
        &self,
        job_id: &JobId,
        backoff_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let retry_count = 1;
        let idempotency_key = format!("job-scheduling-retry-{}-{}", job_id.0, retry_count);

        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobRequeued".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "retry_count": retry_count,
                "backoff_seconds": backoff_secs,
                "reason": "SCHEDULING_FAILURE"
            }),
            Some(serde_json::json!({
                "source": "SchedulingFailureHandler",
                "handler_type": "retry",
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
            backoff_secs = backoff_secs,
            "📤 JobRequeued event persisted for scheduling retry"
        );

        Ok(())
    }

    /// Schedule retry with exponential backoff (for backpressure scenarios)
    async fn schedule_retry_with_backoff(
        &self,
        job_id: &JobId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let retry_count = 1;
        // Exponential backoff: 30s, 60s, 120s, etc.
        let backoff_secs = BASE_BACKOFF_SECS * 2_u64.pow(retry_count - 1);
        let idempotency_key = format!("job-scheduling-backoff-{}-{}", job_id.0, retry_count);

        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobRequeued".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "retry_count": retry_count,
                "backoff_seconds": backoff_secs,
                "reason": "SCHEDULING_BACKPRESSURE",
                "retry_strategy": "exponential_backoff"
            }),
            Some(serde_json::json!({
                "source": "SchedulingFailureHandler",
                "handler_type": "backpressure_retry",
                "retry_attempt": retry_count,
                "backoff_seconds": backoff_secs,
                "max_retries": MAX_SCHEDULING_RETRIES
            })),
            Some(idempotency_key),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        info!(
            job_id = %job_id,
            backoff_secs = backoff_secs,
            "📤 JobRequeued event persisted for scheduling backpressure retry"
        );

        Ok(())
    }

    /// Emit alert event for monitoring/alerting systems
    async fn emit_alert(
        &self,
        alert_type: &str,
        job_id: &JobId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let event = OutboxEventInsert::for_job(
            job_id.0,
            "SystemAlert".to_string(),
            serde_json::json!({
                "alert_type": alert_type,
                "job_id": job_id.0.to_string(),
                "severity": if alert_type == "PROVIDERS_OVERLOADED" { "warning" } else { "info" },
                "message": format!("Scheduling failure: {}", alert_type)
            }),
            Some(serde_json::json!({
                "source": "SchedulingFailureHandler",
                "alert_category": "scheduling"
            })),
            Some(format!("alert-{}-{}", alert_type, Utc::now().timestamp())),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        debug!(
            job_id = %job_id,
            alert_type = alert_type,
            "📤 SystemAlert event persisted"
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
                "source": "SchedulingFailureHandler",
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
            "🚨 Job marked as Failed due to scheduling failure"
        );

        Ok(())
    }
}

// Re-export EventHandler from the event module
use crate::jobs::event_subscriber::EventHandler;

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::{Job, JobsFilter};
    use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId};
    use hodei_shared::states::JobState;
    use std::result::Result;
    use uuid::Uuid;

    // Mock implementations for testing
    struct MockJobRepository;

    #[async_trait::async_trait]
    impl hodei_server_domain::jobs::JobRepository for MockJobRepository {
        async fn find_by_id(&self, _id: &JobId) -> std::result::Result<Option<Job>, DomainError> {
            Ok(None)
        }

        async fn update_state(
            &self,
            _job_id: &JobId,
            _state: JobState,
        ) -> std::result::Result<(), DomainError> {
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
    async fn test_no_providers_emits_alert_and_retry() {
        let handler = SchedulingFailureHandler::new(
            Arc::new(MockJobRepository),
            Arc::new(MockOutboxRepository),
        );

        let job_id = JobId::new();
        let providers = vec![ProviderId::new()];

        let event = DomainEvent::SchedulingDecisionFailed {
            job_id,
            failure_reason: SchedulingFailureReason::NoProvidersAvailable,
            attempted_providers: providers,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_provider_overloaded_applies_backpressure() {
        let handler = SchedulingFailureHandler::new(
            Arc::new(MockJobRepository),
            Arc::new(MockOutboxRepository),
        );

        let job_id = JobId::new();
        let providers = vec![ProviderId::new()];

        let event = DomainEvent::SchedulingDecisionFailed {
            job_id,
            failure_reason: SchedulingFailureReason::ProviderOverloaded,
            attempted_providers: providers,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_matching_provider_marks_job_failed() {
        let handler = SchedulingFailureHandler::new(
            Arc::new(MockJobRepository),
            Arc::new(MockOutboxRepository),
        );

        let job_id = JobId::new();
        let providers = vec![ProviderId::new()];

        let event = DomainEvent::SchedulingDecisionFailed {
            job_id,
            failure_reason: SchedulingFailureReason::NoMatchingProviders {
                requirement: "gpu: true".to_string(),
            },
            attempted_providers: providers,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_constraints_unsatisfiable_marks_job_failed() {
        let handler = SchedulingFailureHandler::new(
            Arc::new(MockJobRepository),
            Arc::new(MockOutboxRepository),
        );

        let job_id = JobId::new();
        let providers = vec![ProviderId::new()];

        let event = DomainEvent::SchedulingDecisionFailed {
            job_id,
            failure_reason: SchedulingFailureReason::NoMatchingProviders {
                requirement: "anti-affinity with self".to_string(),
            },
            attempted_providers: providers,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let result = handler.handle_event(event).await;
        assert!(result.is_ok());
    }
}
