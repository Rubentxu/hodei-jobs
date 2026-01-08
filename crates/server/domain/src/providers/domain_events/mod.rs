//! Provider Domain Events Module
//!
//! Events related to provider lifecycle: registration, health, and recovery.
//! This module implements the Providers bounded context for domain events.
//!
//! Events are defined in providers/events.rs and re-exported here for consistency.

pub use super::ProviderRegistered;
pub use super::ProviderUpdated;
pub use super::ProviderHealthChanged;
pub use super::ProviderRecovered;
pub use super::JobQueueDepthChanged;
pub use super::AutoScalingTriggered;
pub use super::ProviderSelected;
pub use super::ProviderExecutionError;
pub use super::SchedulingDecisionFailed;
