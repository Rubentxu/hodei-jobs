use hodei_jobs_domain::audit::{AuditLog, AuditRepository};
use hodei_jobs_domain::events::DomainEvent;
use hodei_jobs_domain::shared_kernel::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct AuditService {
    repository: Arc<dyn AuditRepository>,
}

impl AuditService {
    pub fn new(repository: Arc<dyn AuditRepository>) -> Self {
        Self { repository }
    }

    pub async fn log_event(&self, event: &DomainEvent) -> Result<()> {
        let event_type = event.event_type().to_string();

        let payload = serde_json::to_value(event).unwrap_or_default();
        let correlation_id = event.correlation_id();
        let actor = event.actor().or_else(|| Some("system".to_string()));

        let audit_log = AuditLog::new(event_type, payload, correlation_id, actor);

        self.repository.save(&audit_log).await?;
        Ok(())
    }

    pub async fn get_logs_by_correlation_id(&self, correlation_id: &str) -> Result<Vec<AuditLog>> {
        self.repository.find_by_correlation_id(correlation_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use hodei_jobs_domain::audit::{AuditLog, AuditRepository};
    use hodei_jobs_domain::events::DomainEvent;
    use hodei_jobs_domain::job_execution::JobSpec;
    use hodei_jobs_domain::events::TerminationReason;
    use hodei_jobs_domain::shared_kernel::{JobId, ProviderId, Result, WorkerId};
    use std::sync::{Arc, Mutex};

    struct MockAuditRepository {
        pub saved_logs: Arc<Mutex<Vec<AuditLog>>>,
    }

    impl MockAuditRepository {
        fn new() -> Self {
            Self {
                saved_logs: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl AuditRepository for MockAuditRepository {
        async fn save(&self, log: &AuditLog) -> Result<()> {
            self.saved_logs.lock().unwrap().push(log.clone());
            Ok(())
        }

        async fn find_by_correlation_id(&self, id: &str) -> Result<Vec<AuditLog>> {
            let logs = self.saved_logs.lock().unwrap();
            Ok(logs
                .iter()
                .filter(|l| l.correlation_id.as_deref() == Some(id))
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn test_log_event_job_created() {
        let repo = Arc::new(MockAuditRepository::new());
        let service = AuditService::new(repo.clone());

        let job_id = JobId::new();
        let expected_correlation_id = job_id.0.to_string();

        let event = DomainEvent::JobCreated {
            job_id: job_id,
            spec: JobSpec::new(vec!["echo".to_string(), "hello".to_string()]),
            occurred_at: Utc::now(),
            correlation_id: Some(expected_correlation_id.clone()),
            actor: Some("test-actor".to_string()),
        };

        service
            .log_event(&event)
            .await
            .expect("Failed to log event");

        let logs = repo.saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_type, "JobCreated");
        assert_eq!(
            logs[0].correlation_id.as_deref(),
            Some(expected_correlation_id.as_str())
        );
        assert_eq!(logs[0].actor.as_deref(), Some("test-actor"));
    }

    #[tokio::test]
    async fn test_log_event_worker_terminated() {
        let repo = Arc::new(MockAuditRepository::new());
        let service = AuditService::new(repo.clone());

        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let correlation_id = "terminate-123".to_string();

        let event = DomainEvent::WorkerTerminated {
            worker_id,
            provider_id,
            reason: TerminationReason::IdleTimeout,
            occurred_at: Utc::now(),
            correlation_id: Some(correlation_id.clone()),
            actor: Some("lifecycle-manager".to_string()),
        };

        service
            .log_event(&event)
            .await
            .expect("Failed to log event");

        let logs = repo.saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_type, "WorkerTerminated");
        assert_eq!(logs[0].correlation_id.as_deref(), Some(correlation_id.as_str()));
        assert_eq!(logs[0].actor.as_deref(), Some("lifecycle-manager"));
    }

    #[tokio::test]
    async fn test_log_event_worker_disconnected() {
        let repo = Arc::new(MockAuditRepository::new());
        let service = AuditService::new(repo.clone());

        let worker_id = WorkerId::new();

        let event = DomainEvent::WorkerDisconnected {
            worker_id,
            last_heartbeat: Some(Utc::now()),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        service
            .log_event(&event)
            .await
            .expect("Failed to log event");

        let logs = repo.saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_type, "WorkerDisconnected");
        assert_eq!(logs[0].actor.as_deref(), Some("system")); // Default actor
    }

    #[tokio::test]
    async fn test_log_event_worker_provisioned() {
        let repo = Arc::new(MockAuditRepository::new());
        let service = AuditService::new(repo.clone());

        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();

        let event = DomainEvent::WorkerProvisioned {
            worker_id,
            provider_id,
            spec_summary: "image=hodei-worker:latest".to_string(),
            occurred_at: Utc::now(),
            correlation_id: Some("provision-456".to_string()),
            actor: Some("scheduler".to_string()),
        };

        service
            .log_event(&event)
            .await
            .expect("Failed to log event");

        let logs = repo.saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_type, "WorkerProvisioned");
    }

    #[tokio::test]
    async fn test_get_audit_logs_by_correlation_id() {
        let repo = Arc::new(MockAuditRepository::new());
        let service = AuditService::new(repo.clone());
        let correlation_id = "test-correlation-id";

        // Seed some logs
        let log1 = AuditLog::new(
            "JobCreated".to_string(),
            serde_json::Value::Null,
            Some(correlation_id.to_string()),
            Some("actor1".to_string()),
        );
        let log2 = AuditLog::new(
            "JobStarted".to_string(),
            serde_json::Value::Null,
            Some(correlation_id.to_string()),
            Some("actor1".to_string()),
        );
        let log3 = AuditLog::new(
            "OtherEvent".to_string(),
            serde_json::Value::Null,
            Some("other-id".to_string()),
            None,
        );

        repo.save(&log1).await.expect("Failed to save log1");
        repo.save(&log2).await.expect("Failed to save log2");
        repo.save(&log3).await.expect("Failed to save log3");

        // Execute Use Case
        let result = service
            .get_logs_by_correlation_id(correlation_id)
            .await
            .expect("Failed to get logs");

        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|l| l.event_type == "JobCreated"));
        assert!(result.iter().any(|l| l.event_type == "JobStarted"));
    }
}
