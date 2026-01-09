//! Worker Domain Events Module
//!
//! Events related to worker lifecycle: registration, heartbeat, and termination.
//! This module implements the Workers bounded context for domain events.
//!
//! Events are defined in workers/events.rs and re-exported here for consistency.

pub use super::super::events::CleanupReason;
pub use super::super::events::TerminationReason;

// Re-export all worker events from the main events module
pub use super::WorkerStatusChanged;
pub use super::WorkerTerminated;
pub use super::WorkerDisconnected;
pub use super::WorkerProvisioned;
pub use super::WorkerReconnected;
pub use super::WorkerRecoveryFailed;
pub use super::WorkerReadyForJob;
pub use super::WorkerProvisioningRequested;
pub use super::WorkerHeartbeat;
pub use super::WorkerReady;
pub use super::WorkerStateUpdated;
pub use super::WorkerSelfTerminated;
pub use super::WorkerProvisioningError;
pub use super::WorkerEphemeralCreated;
pub use super::WorkerEphemeralReady;
pub use super::WorkerEphemeralTerminating;
pub use super::WorkerEphemeralTerminated;
pub use super::WorkerEphemeralCleanedUp;
pub use super::OrphanWorkerDetected;
pub use super::GarbageCollectionCompleted;
pub use super::WorkerEphemeralIdle;
