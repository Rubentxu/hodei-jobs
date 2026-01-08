//! Jobs Bounded Context
//!
//! Maneja el lifecycle de jobs, especificaciones, plantillas y coordinación

pub mod aggregate;
pub mod coordination;
pub mod domain_events;
pub mod events;
pub mod templates;

pub use aggregate::*;
pub use coordination::*;
pub use domain_events::*;
pub use events::*;
pub use templates::*;

// Re-export all events at the jobs module level for convenience
pub use self::events::{
    JobAccepted, JobAssigned, JobCancelled, JobCreated, JobDispatchAcknowledged,
    JobDispatchFailed, JobExecutionError, JobQueued, JobRetried, JobStatusChanged,
    RunJobReceived,
};
