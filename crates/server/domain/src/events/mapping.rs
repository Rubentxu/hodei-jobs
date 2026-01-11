//! Event Mapping for Realtime Projections
//!
//! Provides projection from DomainEvents to ClientEvents for WebSocket communication.
//!
//! This module solves Connascence of Name by separating DomainEvents
//! (40+ internal events) from ClientEvents (~10 UI-relevant events).
//!
//! ## Design Decisions
//!
//! - **Filtering**: Internal events are filtered out (return None)
//! - **Projection**: Removes internal fields (correlation_id, actor)
//! - **Sanitization**: Removes sensitive data (JobSpec with commands)
//! - **Topic Determination**: Maps events to subscription topics

use crate::events::DomainEvent;
use crate::shared_kernel::{JobId, JobState, ProviderId, WorkerId, WorkerState};
use chrono::Utc;

/// Client-side event projected from DomainEvent
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClientEvent {
    pub event_type: String,
    pub event_version: u32,
    pub aggregate_id: String,
    pub payload: hodei_shared::realtime::ClientEventPayload,
    pub occurred_at: i64,
}

impl ClientEvent {
    pub fn from_domain_event(event: DomainEvent) -> Option<Self> {
        use hodei_shared::realtime::ClientEventPayload as Payload;
        use hodei_shared::realtime::ClientEventPayload::*;

        match event {
            // Job Events
            DomainEvent::JobCreated {
                job_id,
                occurred_at,
                ..
            } => Some(Self {
                event_type: "job.created".to_string(),
                event_version: 1,
                aggregate_id: job_id.to_string(),
                payload: Payload::JobCreated {
                    job_id: job_id.to_string(),
                    timestamp: occurred_at.timestamp_millis(),
                },
                occurred_at: occurred_at.timestamp_millis(),
            }),

            DomainEvent::JobStatusChanged {
                job_id,
                old_state,
                new_state,
                occurred_at,
                ..
            } => Some(Self {
                event_type: "job.status_changed".to_string(),
                event_version: 1,
                aggregate_id: job_id.to_string(),
                payload: Payload::JobStatusChanged {
                    job_id: job_id.to_string(),
                    old_status: old_state.to_string(),
                    new_status: new_state.to_string(),
                    timestamp: occurred_at.timestamp_millis(),
                },
                occurred_at: occurred_at.timestamp_millis(),
            }),

            // Worker Events
            DomainEvent::WorkerReady {
                worker_id,
                provider_id,
                job_id,
                ready_at,
                ..
            } => Some(Self {
                event_type: "worker.ready".to_string(),
                event_version: 1,
                aggregate_id: worker_id.to_string(),
                payload: Payload::WorkerReady {
                    worker_id: worker_id.to_string(),
                    provider_id: provider_id.to_string(),
                    current_job_id: job_id.map(|id| id.to_string()),
                    timestamp: ready_at.timestamp_millis(),
                },
                occurred_at: ready_at.timestamp_millis(),
            }),

            DomainEvent::WorkerHeartbeat {
                worker_id,
                state,
                load_average,
                memory_usage_mb,
                current_job_id,
                occurred_at,
                ..
            } => Some(Self {
                event_type: "worker.heartbeat".to_string(),
                event_version: 1,
                aggregate_id: worker_id.to_string(),
                payload: Payload::WorkerHeartbeat {
                    worker_id: worker_id.to_string(),
                    state: state.to_string(),
                    cpu: load_average.map(|l| l as f32),
                    memory_mb: memory_usage_mb,
                    active_jobs: if current_job_id.is_some() { 1 } else { 0 },
                    timestamp: occurred_at.timestamp_millis(),
                },
                occurred_at: occurred_at.timestamp_millis(),
            }),

            // Filter internal events
            DomainEvent::JobDispatchAcknowledged { .. }
            | DomainEvent::RunJobReceived { .. }
            | DomainEvent::JobQueued { .. }
            | DomainEvent::JobQueueDepthChanged { .. }
            | DomainEvent::WorkerStatusChanged { .. }
            | DomainEvent::WorkerProvisioned { .. }
            | DomainEvent::WorkerProvisioningRequested { .. }
            | DomainEvent::WorkerProvisioningError { .. }
            | DomainEvent::WorkerReadyForJob { .. }
            | DomainEvent::WorkerStateUpdated { .. }
            | DomainEvent::WorkerEphemeralCreated { .. }
            | DomainEvent::WorkerEphemeralReady { .. }
            | DomainEvent::WorkerEphemeralTerminating { .. }
            | DomainEvent::WorkerEphemeralTerminated { .. }
            | DomainEvent::WorkerEphemeralCleanedUp { .. }
            | DomainEvent::WorkerEphemeralIdle { .. }
            | DomainEvent::OrphanWorkerDetected { .. }
            | DomainEvent::GarbageCollectionCompleted { .. }
            | DomainEvent::WorkerRecoveryFailed { .. }
            | DomainEvent::WorkerSelfTerminated { .. }
            | DomainEvent::SchedulingDecisionFailed { .. } => None,
        }
    }

    pub fn topics(&self) -> Vec<String> {
        let mut topics = vec![];
        topics.push(format!("agg:{}", self.aggregate_id));

        match self.event_type.as_str() {
            "job.created" | "job.status_changed" => {
                topics.push("jobs:all".to_string());
            }
            "worker.ready" | "worker.heartbeat" => {
                topics.push("workers:all".to_string());
            }
            _ => {}
        }

        topics
    }
}
