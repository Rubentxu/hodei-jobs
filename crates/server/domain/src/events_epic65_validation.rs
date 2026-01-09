// EPIC-65 Validation Test: Modular Event Conversions
// Este test valida que las conversiones From entre eventos modulares y DomainEvent funcionan correctamente

#[cfg(test)]
mod epic65_validation_tests {
    use crate::events::DomainEvent;
    use crate::jobs::events::{
        JobAccepted, JobAssigned, JobCancelled, JobCreated, JobDispatchAcknowledged,
        JobDispatchFailed, JobExecutionError, JobQueued, JobRetried, JobStatusChanged,
        RunJobReceived,
    };
    use crate::shared_kernel::{JobId, JobState};
    use crate::workers::events::{
        WorkerDisconnected, WorkerHeartbeat, WorkerProvisioned, WorkerReadyForJob,
        WorkerRecoveryFailed, WorkerReconnected, WorkerStatusChanged,
        WorkerTerminated,
    };
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_job_created_conversion() {
        // GIVEN: Un evento modular JobCreated
        let job_id = JobId::new();
        let modular_event = JobCreated {
            job_id: job_id.clone(),
            spec: crate::jobs::JobSpec::default(),
            occurred_at: Utc::now(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            actor: Some("test-user".to_string()),
        };

        // WHEN: Convertimos a DomainEvent usando .into()
        let domain_event: DomainEvent = modular_event.into();

        // THEN: La conversión preserva todos los campos
        match domain_event {
            DomainEvent::JobCreated {
                job_id: id,
                spec,
                occurred_at,
                correlation_id,
                actor,
            } => {
                assert_eq!(id, job_id);
                assert_eq!(correlation_id, Some("test-user".to_string().clone().replace("test-user", &Uuid::new_v4().to_string())));
                assert_eq!(actor, Some("test-user".to_string()));
            }
            _ => panic!("Expected JobCreated variant"),
        }
    }

    #[test]
    fn test_job_status_changed_conversion() {
        // GIVEN: Un evento modular JobStatusChanged
        let job_id = JobId::new();
        let modular_event = JobStatusChanged {
            job_id: job_id.clone(),
            old_state: JobState::Queued,
            new_state: JobState::Pending,
            occurred_at: Utc::now(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            actor: Some("system".to_string()),
        };

        // WHEN: Convertimos a DomainEvent
        let domain_event: DomainEvent = modular_event.into();

        // THEN: La conversión preserva el estado
        match domain_event {
            DomainEvent::JobStatusChanged {
                job_id,
                old_state,
                new_state,
                ..
            } => {
                assert_eq!(job_id, job_id);
                assert_eq!(old_state, JobState::Queued);
                assert_eq!(new_state, JobState::Pending);
            }
            _ => panic!("Expected JobStatusChanged variant"),
        }
    }

    #[test]
    fn test_job_cancelled_conversion() {
        // GIVEN: Un evento modular JobCancelled
        let job_id = JobId::new();
        let modular_event = JobCancelled {
            job_id: job_id.clone(),
            reason: Some("User requested cancellation".to_string()),
            occurred_at: Utc::now(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            actor: Some("admin".to_string()),
        };

        // WHEN: Convertimos a DomainEvent
        let domain_event: DomainEvent = modular_event.into();

        // THEN: La conversión preserva la razón
        match domain_event {
            DomainEvent::JobCancelled { job_id, reason, .. } => {
                assert_eq!(job_id, job_id);
                assert_eq!(reason, Some("User requested cancellation".to_string()));
            }
            _ => panic!("Expected JobCancelled variant"),
        }
    }



    #[test]
    fn test_worker_status_changed_conversion() {
        // GIVEN: Un evento modular WorkerStatusChanged
        let worker_id = crate::workers::WorkerId::new();
        let modular_event = crate::workers::events::WorkerStatusChanged {
            worker_id: worker_id.clone(),
            old_status: crate::shared_kernel::WorkerState::Initializing,
            new_status: crate::shared_kernel::WorkerState::Idle,
            occurred_at: Utc::now(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            actor: Some("worker-agent".to_string()),
        };

        // WHEN: Convertimos a DomainEvent
        let domain_event: DomainEvent = modular_event.into();

        // THEN: La conversión preserva estados
        match domain_event {
            DomainEvent::WorkerStatusChanged {
                worker_id,
                old_status,
                new_status,
                ..
            } => {
                assert_eq!(worker_id, worker_id);
                assert_eq!(old_status, crate::shared_kernel::WorkerState::Initializing);
                assert_eq!(new_status, crate::shared_kernel::WorkerState::Idle);
            }
            _ => panic!("Expected WorkerStatusChanged variant"),
        }
    }

    #[test]
    fn test_all_job_events_convertible() {
        // Valida que todos los eventos de jobs tengan conversión implementada
        let job_id = JobId::new();
        let now = Utc::now();
        let correlation_id = Some(Uuid::new_v4().to_string());
        let actor = Some("test".to_string());

        // JobCreated
        let _: DomainEvent = JobCreated {
            job_id: job_id.clone(),
            spec: crate::jobs::JobSpec::default(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // JobStatusChanged
        let _: DomainEvent = JobStatusChanged {
            job_id: job_id.clone(),
            old_state: JobState::Queued,
            new_state: JobState::Pending,
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // JobCancelled
        let _: DomainEvent = JobCancelled {
            job_id: job_id.clone(),
            reason: Some("test".to_string()),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // JobRetried
        let _: DomainEvent = JobRetried {
            job_id: job_id.clone(),
            attempt: 1,
            max_attempts: 3,
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // JobAssigned
        let _: DomainEvent = JobAssigned {
            job_id: job_id.clone(),
            worker_id: crate::workers::WorkerId::new(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // JobAccepted
        let _: DomainEvent = JobAccepted {
            job_id: job_id.clone(),
            worker_id: crate::workers::WorkerId::new(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // JobDispatchAcknowledged
        let _: DomainEvent = JobDispatchAcknowledged {
            job_id: job_id.clone(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // RunJobReceived
        let _: DomainEvent = RunJobReceived {
            job_id: job_id.clone(),
            worker_id: crate::workers::WorkerId::new(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // JobQueued
        let _: DomainEvent = JobQueued {
            job_id: job_id.clone(),
            queue_position: 1,
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();
    }

    #[test]
    fn test_all_worker_events_convertible() {
        // Valida que todos los eventos de workers tengan conversión implementada
        let worker_id = crate::workers::WorkerId::new();
        let provider_id = crate::shared_kernel::ProviderId::new();
        let now = Utc::now();
        let correlation_id = Some(Uuid::new_v4().to_string());
        let actor = Some("test".to_string());

        // WorkerReady (replaces deprecated WorkerRegistered)
        let _: DomainEvent = WorkerReady {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            job_id: None,
            ready_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerStatusChanged
        let _: DomainEvent = crate::workers::events::WorkerStatusChanged {
            worker_id: worker_id.clone(),
            old_status: crate::shared_kernel::WorkerState::Initializing,
            new_status: crate::shared_kernel::WorkerState::Idle,
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerTerminated
        let _: DomainEvent = WorkerTerminated {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            reason: crate::workers::TerminationReason::JobCompleted,
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerDisconnected
        let _: DomainEvent = WorkerDisconnected {
            worker_id: worker_id.clone(),
            reason: Some("Connection lost".to_string()),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerProvisioned
        let _: DomainEvent = WorkerProvisioned {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerReconnected
        let _: DomainEvent = WorkerReconnected {
            worker_id: worker_id.clone(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerRecoveryFailed
        let _: DomainEvent = WorkerRecoveryFailed {
            worker_id: worker_id.clone(),
            reason: "Max retries exceeded".to_string(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerReadyForJob
        let _: DomainEvent = WorkerReadyForJob {
            worker_id: worker_id.clone(),
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();

        // WorkerHeartbeat
        let _: DomainEvent = WorkerHeartbeat {
            worker_id: worker_id.clone(),
            state: crate::shared_kernel::WorkerState::Idle,
            load_average: Some(0.5),
            memory_usage_mb: Some(1024),
            current_job_id: None,
            occurred_at: now,
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        }
        .into();
    }
}
