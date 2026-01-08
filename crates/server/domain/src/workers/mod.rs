//! Workers Bounded Context
//!
//! Maneja el registro y gestión de workers

pub mod aggregate;
pub mod auto_scaling;
pub mod domain_events;
pub mod events;
pub mod health;
pub mod provider_api;
pub mod provisioning;
pub mod registry;

pub use aggregate::*;
pub use auto_scaling::*;
pub use domain_events::*;
pub use events::*;
pub use health::*;
pub use provider_api::*;
pub use provisioning::*;
pub use registry::*;

// Re-export all events at the workers module level for convenience
pub use self::events::{
    GarbageCollectionCompleted, OrphanWorkerDetected, WorkerDisconnected,
    WorkerEphemeralCleanedUp, WorkerEphemeralCreated, WorkerEphemeralIdle,
    WorkerEphemeralReady, WorkerEphemeralTerminated, WorkerEphemeralTerminating,
    WorkerHeartbeat, WorkerProvisioned, WorkerReady, WorkerReadyForJob, WorkerReconnected,
    WorkerRecoveryFailed, WorkerRegistered, WorkerSelfTerminated, WorkerStateUpdated,
    WorkerStatusChanged, WorkerTerminated,
};
