//! Job Domain Events Module
//!
//! Events related to job lifecycle: creation, execution, and completion.
//! This module implements the Jobs bounded context for domain events.
//!
//! Events are defined in jobs/events.rs and re-exported here for consistency.

pub use super::super::events::CleanupReason;
pub use super::super::events::TerminationReason;

// Re-export all job events from the main events module
pub use super::JobCreated;
pub use super::JobStatusChanged;
pub use super::JobCancelled;
pub use super::JobRetried;
pub use super::JobAssigned;
pub use super::JobAccepted;
pub use super::JobDispatchAcknowledged;
pub use super::RunJobReceived;
pub use super::JobQueued;
pub use super::JobExecutionError;
pub use super::JobDispatchFailed;
