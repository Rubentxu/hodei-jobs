//! TDD Tests for Event Mapping
//!
//! Following Red-Green-Refactor cycle

#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use hodei_server_domain::events::mapping::ClientEvent;
#[cfg(test)]
use hodei_server_domain::events::DomainEvent;
#[cfg(test)]
use hodei_server_domain::jobs::JobSpec;
#[cfg(test)]
use hodei_server_domain::shared_kernel::{JobId, JobState, ProviderId, WorkerId, WorkerState};

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use hodei_server_domain::events::mapping::ClientEvent;
    use hodei_server_domain::events::DomainEvent;
    use hodei_server_domain::jobs::JobSpec;
    use hodei_server_domain::shared_kernel::{JobId, JobState, ProviderId, WorkerId, WorkerState};

    // Test 1: Internal Events Are Filtered
    #[test]
    fn test_internal_event_job_dispatch_acknowledged_filtered() {
        let event = DomainEvent::JobDispatchAcknowledged {
            job_id: JobId::new(),
            worker_id: WorkerId::new(),
            acknowledged_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = ClientEvent::from_domain_event(event);

        assert!(
            client_event.is_none(),
            "JobDispatchAcknowledged should be filtered"
        );
    }

    #[test]
    fn test_internal_event_run_job_received_filtered() {
        let event = DomainEvent::RunJobReceived {
            job_id: JobId::new(),
            worker_id: WorkerId::new(),
            received_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = ClientEvent::from_domain_event(event);

        assert!(client_event.is_none(), "RunJobReceived should be filtered");
    }

    #[test]
    fn test_internal_event_queue_depth_changed_filtered() {
        let event = DomainEvent::JobQueueDepthChanged {
            queue_depth: 100,
            threshold: 50,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = ClientEvent::from_domain_event(event);

        assert!(
            client_event.is_none(),
            "JobQueueDepthChanged should be filtered"
        );
    }

    // Test 2: Sensitive Data Is Removed
    #[test]
    fn test_sensitive_data_removed_from_job_created() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec![
            "echo".to_string(),
            "secret_password".to_string(),
            "--config".to_string(),
            "/path/to/secret.config".to_string(),
        ]);

        let event = DomainEvent::JobCreated {
            job_id: job_id.clone(),
            spec,
            occurred_at: Utc::now(),
            correlation_id: Some("correlation-123".to_string()),
            actor: Some("system".to_string()),
        };

        let client_event = ClientEvent::from_domain_event(event).unwrap();

        let payload_json = serde_json::to_string(&client_event.payload).unwrap();

        assert!(!payload_json.contains("secret_password"));
        assert!(!payload_json.contains("secret.config"));
        assert!(!payload_json.contains("correlation-123"));
        assert!(!payload_json.contains("system"));
        assert!(payload_json.contains(&job_id.to_string()));
    }

    // Test 3: Job Events Determine Correct Topics
    #[test]
    fn test_job_status_changed_topics() {
        let job_id = JobId::new();
        let event = DomainEvent::JobStatusChanged {
            job_id: job_id.clone(),
            old_state: JobState::Queued,
            new_state: JobState::Running,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = ClientEvent::from_domain_event(event).unwrap();

        assert!(client_event.event_type == "job.status_changed");
        assert_eq!(client_event.aggregate_id, job_id.to_string());
        assert!(client_event.topics().contains(&format!("agg:{}", job_id)));
        assert!(client_event.topics().contains(&"jobs:all".to_string()));
    }

    // Test 4: Worker Events Determine Correct Topics
    #[test]
    fn test_worker_heartbeat_topics() {
        let worker_id = WorkerId::new();
        let event = DomainEvent::WorkerHeartbeat {
            worker_id: worker_id.clone(),
            state: WorkerState::Ready,
            load_average: Some(1.5),
            memory_usage_mb: Some(512),
            current_job_id: None,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = ClientEvent::from_domain_event(event).unwrap();

        assert!(client_event.event_type == "worker.heartbeat");
        assert_eq!(client_event.aggregate_id, worker_id.to_string());
        assert!(client_event
            .topics()
            .contains(&format!("agg:{}", worker_id)));
        assert!(client_event.topics().contains(&"workers:all".to_string()));
    }

    // Test 5: Event Serialization
    #[test]
    fn test_client_event_serialization() {
        let event = DomainEvent::JobStatusChanged {
            job_id: JobId::new(),
            old_state: JobState::Queued,
            new_state: JobState::Running,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = ClientEvent::from_domain_event(event).unwrap();

        let json = serde_json::to_string(&client_event).unwrap();

        assert!(json.contains("event_type"));
        assert!(json.contains("event_version"));
        assert!(json.contains("aggregate_id"));
        assert!(json.contains("payload"));
        assert!(json.contains("occurred_at"));
    }
}
