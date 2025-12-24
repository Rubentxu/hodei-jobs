//! Scheduling Service - Application layer wrapper for scheduling
//!
//! This module provides an application-level service wrapper for the domain SmartScheduler.
//! It maintains the domain layer clean while providing application-specific functionality.

pub use hodei_server_domain::scheduling::{
    SchedulerConfig, SchedulingContext, SchedulingDecision, SmartScheduler,
};

use hodei_server_domain::scheduling::JobScheduler;

/// Scheduling Service - Coordina scheduling con registry y providers
pub struct SchedulingService {
    scheduler: SmartScheduler,
}

impl SchedulingService {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            scheduler: SmartScheduler::new(config),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(SchedulerConfig::default())
    }

    /// Get the underlying scheduler
    pub fn scheduler(&self) -> &SmartScheduler {
        &self.scheduler
    }

    /// Make a scheduling decision
    pub async fn make_decision(
        &self,
        context: SchedulingContext,
    ) -> anyhow::Result<SchedulingDecision> {
        self.scheduler.schedule(context).await
    }

    /// Select a provider considering job preferences
    pub fn select_provider_with_preferences(
        &self,
        job: &hodei_server_domain::jobs::Job,
        providers: &[hodei_server_domain::scheduling::ProviderInfo],
    ) -> Option<hodei_server_domain::shared_kernel::ProviderId> {
        self.scheduler
            .select_provider_with_preferences(job, providers)
    }
}
