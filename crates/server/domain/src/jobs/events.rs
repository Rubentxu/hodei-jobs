//! Job Domain Events
//!
//! Events related to job lifecycle: creation, execution, and completion.
//! This module implements the Jobs bounded context for domain events.

use crate::jobs::JobSpec;
use crate::shared_kernel::{JobId, JobState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Event published when a new job is created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobCreated {
    pub job_id: JobId,
    pub spec: JobSpec,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when job state changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobStatusChanged {
    pub job_id: JobId,
    pub old_state: JobState,
    pub new_state: JobState,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when a job is explicitly cancelled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobCancelled {
    pub job_id: JobId,
    pub reason: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when a job is retried
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobRetried {
    pub job_id: JobId,
    pub attempt: u32,
    pub max_attempts: u32,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when a job is assigned to a worker
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobAssigned {
    pub job_id: JobId,
    pub worker_id: crate::shared_kernel::WorkerId,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when a worker acknowledges job receipt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobAccepted {
    pub job_id: JobId,
    pub worker_id: crate::shared_kernel::WorkerId,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event confirming job dispatch at transport level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobDispatchAcknowledged {
    pub job_id: JobId,
    pub worker_id: crate::shared_kernel::WorkerId,
    pub acknowledged_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event when worker receives RUN_JOB command
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunJobReceived {
    pub job_id: JobId,
    pub worker_id: crate::shared_kernel::WorkerId,
    pub received_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// EPIC-29: Job queued for reactive dispatch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobQueued {
    pub job_id: JobId,
    pub preferred_provider: Option<crate::shared_kernel::ProviderId>,
    pub job_requirements: JobSpec,
    pub queued_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event when job execution fails with detailed error info
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobExecutionError {
    pub job_id: JobId,
    pub worker_id: crate::shared_kernel::WorkerId,
    pub failure_reason: hodei_shared::states::JobFailureReason,
    pub exit_code: i32,
    pub command: String,
    pub arguments: Vec<String>,
    pub working_dir: Option<String>,
    pub execution_time_ms: u64,
    pub suggested_actions: Vec<String>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event when job dispatch to worker fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobDispatchFailed {
    pub job_id: JobId,
    pub worker_id: crate::shared_kernel::WorkerId,
    pub failure_reason: hodei_shared::states::DispatchFailureReason,
    pub retry_count: u32,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}
