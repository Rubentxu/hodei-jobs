//! Worker Domain Events
//!
//! Events related to worker lifecycle: registration, heartbeat, and termination.
//! This module implements the Workers bounded context for domain events.

use crate::shared_kernel::{JobId, WorkerId, WorkerState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Event published when worker state changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerStatusChanged {
    pub worker_id: WorkerId,
    pub old_status: WorkerState,
    pub new_status: WorkerState,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when a worker is terminated
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerTerminated {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub reason: crate::events::TerminationReason,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when worker disconnects unexpectedly
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerDisconnected {
    pub worker_id: WorkerId,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when worker is provisioned
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerProvisioned {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub spec_summary: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when worker reconnects and recovers session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerReconnected {
    pub worker_id: WorkerId,
    pub session_id: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when worker recovery fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerRecoveryFailed {
    pub worker_id: WorkerId,
    pub invalid_session_id: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// EPIC-29: Worker ready to receive job assignment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerReadyForJob {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub capabilities: Vec<String>,
    pub tags: HashMap<String, String>,
    pub ready_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// EPIC-29: Worker provisioning request for a specific job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerProvisioningRequested {
    pub job_id: JobId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub job_requirements: crate::jobs::JobSpec,
    pub requested_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// EPIC-29: Worker heartbeat replaces polling-based monitoring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerHeartbeat {
    pub worker_id: WorkerId,
    pub state: WorkerState,
    pub load_average: Option<f64>,
    pub memory_usage_mb: Option<u64>,
    pub current_job_id: Option<JobId>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Worker ready to receive jobs (available for assignment)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerReady {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    /// Job ID to dispatch to this worker (None for general availability)
    pub job_id: Option<JobId>,
    pub ready_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Complete worker state snapshot for audit/reconciliation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerStateUpdated {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub old_state: WorkerState,
    pub new_state: WorkerState,
    pub current_job_id: Option<JobId>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub transition_reason: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Worker self-terminates after post-job cleanup timeout
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerSelfTerminated {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub last_job_id: Option<JobId>,
    pub expected_cleanup_ms: u64,
    pub actual_wait_ms: u64,
    pub worker_state: WorkerState,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Worker provisioning error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerProvisioningError {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub failure_reason: hodei_shared::states::ProvisioningFailureReason,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

// =============================================================================
// Ephemeral Worker Events (EPIC-26)
// =============================================================================

/// Event published when ephemeral worker is created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerEphemeralCreated {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub max_lifetime_secs: u64,
    pub ttl_after_completion_secs: Option<u64>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when ephemeral worker is ready
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerEphemeralReady {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when ephemeral worker starts termination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerEphemeralTerminating {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub reason: crate::events::TerminationReason,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when ephemeral worker completes termination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerEphemeralTerminated {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub cleanup_scheduled: bool,
    pub ttl_expires_at: Option<DateTime<Utc>>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when ephemeral worker is cleaned up by garbage collector
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerEphemeralCleanedUp {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub cleanup_reason: crate::events::CleanupReason,
    pub cleanup_duration_ms: u64,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when orphan worker is detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrphanWorkerDetected {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub last_seen: DateTime<Utc>,
    pub orphaned_duration_secs: u64,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when garbage collection cycle completes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GarbageCollectionCompleted {
    pub provider_id: crate::shared_kernel::ProviderId,
    pub workers_cleaned: usize,
    pub orphans_detected: usize,
    pub errors: usize,
    pub duration_ms: u64,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when ephemeral worker is marked idle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerEphemeralIdle {
    pub worker_id: WorkerId,
    pub provider_id: crate::shared_kernel::ProviderId,
    pub idle_since: DateTime<Utc>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}
