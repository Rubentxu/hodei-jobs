//! Provider Domain Events
//!
//! Events related to provider lifecycle: registration, health, and recovery.
//! This module implements the Providers bounded context for domain events.

use crate::shared_kernel::{ProviderId, ProviderStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Event published when a new provider registers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderRegistered {
    pub provider_id: ProviderId,
    pub provider_type: String,
    pub config_summary: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when a provider is updated
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderUpdated {
    pub provider_id: ProviderId,
    pub changes: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when provider health status changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderHealthChanged {
    pub provider_id: ProviderId,
    pub old_status: ProviderStatus,
    pub new_status: ProviderStatus,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when provider recovers and is operational again
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderRecovered {
    pub provider_id: ProviderId,
    pub previous_status: ProviderStatus,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when job queue depth changes significantly
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobQueueDepthChanged {
    pub queue_depth: u64,
    pub threshold: u64,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event published when auto-scaling is triggered for a provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoScalingTriggered {
    pub provider_id: ProviderId,
    pub reason: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// EPIC-29: Provider selected for job provisioning
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSelected {
    pub job_id: crate::shared_kernel::JobId,
    pub provider_id: ProviderId,
    pub provider_type: crate::workers::ProviderType,
    pub selection_strategy: String,
    pub effective_cost: f64,
    pub effective_startup_ms: u64,
    pub elapsed_ms: u64,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event when provider execution encounters an error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderExecutionError {
    pub provider_id: ProviderId,
    pub worker_id: crate::shared_kernel::WorkerId,
    pub error_type: hodei_shared::states::ProviderErrorType,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Event when scheduling decision fails for a job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchedulingDecisionFailed {
    pub job_id: crate::shared_kernel::JobId,
    pub failure_reason: hodei_shared::states::SchedulingFailureReason,
    pub attempted_providers: Vec<ProviderId>,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}
