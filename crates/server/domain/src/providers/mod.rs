//! Providers Bounded Context
//!
//! Maneja la configuración de providers

pub mod config;
pub mod domain_events;
pub mod errors;
pub mod events;
pub mod validator;

pub use config::*;
pub use domain_events::*;
pub use errors::*;
pub use events::*;
pub use validator::*;

// Re-export all events at the providers module level for convenience
pub use self::events::{
    AutoScalingTriggered, JobQueueDepthChanged, ProviderExecutionError, ProviderHealthChanged,
    ProviderRecovered, ProviderRegistered, ProviderSelected, ProviderUpdated,
    SchedulingDecisionFailed,
};
