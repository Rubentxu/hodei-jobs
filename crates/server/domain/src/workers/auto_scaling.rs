//! Domain types for Auto-scaling logic.

use crate::shared_kernel::ProviderId;
use serde::{Deserialize, Serialize};

/// Decision made by the auto-scaling logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScalingDecision {
    /// No action needed.
    None,
    /// Request to provision more workers.
    ScaleUp {
        provider_id: ProviderId,
        count: u32,
        reason: String,
    },
    /// Request to terminate idle workers.
    ScaleDown {
        provider_id: ProviderId,
        count: u32,
        reason: String,
    },
}

/// Context information for making scaling decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingContext {
    pub provider_id: ProviderId,
    pub pending_jobs: u32,
    pub healthy_workers: u32,
    pub busy_workers: u32,
    pub degraded_workers: u32,
}

impl ScalingContext {
    pub fn new(
        provider_id: ProviderId,
        pending_jobs: u32,
        healthy_workers: u32,
        busy_workers: u32,
        degraded_workers: u32,
    ) -> Self {
        Self {
            provider_id,
            pending_jobs,
            healthy_workers,
            busy_workers,
            degraded_workers,
        }
    }

    /// Total number of workers currently managed by this context.
    pub fn total_workers(&self) -> u32 {
        self.healthy_workers + self.busy_workers + self.degraded_workers
    }

    /// Workers available to take more work.
    pub fn available_workers(&self) -> u32 {
        self.healthy_workers
    }
}

/// Trait for pluggable auto-scaling strategies.
pub trait AutoScalingStrategy: Send + Sync {
    /// Name of the strategy for logging and identification.
    fn name(&self) -> &str;

    /// Evaluates the context and returns a scaling decision.
    fn evaluate(&self, context: &ScalingContext) -> ScalingDecision;
}
