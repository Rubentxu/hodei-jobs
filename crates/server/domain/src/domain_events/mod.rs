//! Domain Events Module
//!
//! Unified module for all domain events in the Hodei Jobs Platform.
//! Events are organized by bounded context for better maintainability.
//!
//! ## Bounded Contexts
//!
//! - **jobs**: Job lifecycle events (creation, execution, completion)
//! - **workers**: Worker lifecycle events (registration, heartbeat, termination)
//! - **providers**: Provider lifecycle events (registration, health, scaling)
//! - **templates**: EPIC-34 template and scheduled job events
//! - **scheduling**: Scheduling decision events
//!
//! ## Usage
//!
//! ```rust
//! use hodei_server_domain::domain_events::jobs::JobCreated;
//! use hodei_server_domain::domain_events::workers::WorkerRegistered;
//! use hodei_server_domain::domain_events::providers::ProviderHealthChanged;
//! ```
//!
//! ## Migration Strategy
//!
//! This module provides backward compatibility with the legacy `DomainEvent` enum
//! while supporting the new modular event structure. Over time, consumers should
//! migrate to using the specific event types directly.

// Re-export from jobs bounded context
pub mod jobs {
    //! Job lifecycle domain events
    //!
    //! Events related to job creation, execution, status changes, and completion.

    pub use crate::jobs::events::{
        JobAccepted, JobAssigned, JobCancelled, JobCreated, JobDispatchAcknowledged,
        JobDispatchFailed, JobExecutionError, JobQueued, JobRetried, JobStatusChanged,
        RunJobReceived,
    };
}

// Re-export from workers bounded context
pub mod workers {
    //! Worker lifecycle domain events
    //!
    //! Events related to worker registration, heartbeat, provisioning, and termination.

    pub use crate::workers::events::{
        GarbageCollectionCompleted, OrphanWorkerDetected, WorkerDisconnected,
        WorkerEphemeralCleanedUp, WorkerEphemeralCreated, WorkerEphemeralIdle,
        WorkerEphemeralReady, WorkerEphemeralTerminated, WorkerEphemeralTerminating,
        WorkerHeartbeat, WorkerProvisioned, WorkerReady, WorkerReadyForJob, WorkerReconnected,
        WorkerRecoveryFailed, WorkerRegistered, WorkerSelfTerminated, WorkerStateUpdated,
        WorkerStatusChanged, WorkerTerminated,
    };
}

// Re-export from providers bounded context
pub mod providers {
    //! Provider lifecycle domain events
    //!
    //! Events related to provider registration, health monitoring, and scaling.

    pub use crate::providers::events::{
        AutoScalingTriggered, JobQueueDepthChanged, ProviderExecutionError, ProviderHealthChanged,
        ProviderRecovered, ProviderRegistered, ProviderSelected, ProviderUpdated,
        SchedulingDecisionFailed,
    };
}

// Re-export from templates bounded context (EPIC-34)
pub mod templates {
    //! Template and scheduled job domain events (EPIC-34)
    //!
    //! Events related to job templates, scheduled jobs, and execution history.

    pub use crate::templates::events::{
        ExecutionRecorded, ScheduledJobCreated, ScheduledJobError, ScheduledJobMissed,
        ScheduledJobTriggered, TemplateCreated, TemplateDisabled, TemplateRunCreated,
        TemplateUpdated,
    };
}

// Re-export from scheduling bounded context
pub mod scheduling {
    //! Scheduling decision domain events
    //!
    //! Events related to scheduling decisions, worker selection, and provider selection.

    pub use crate::scheduling::events::{
        FilterReason, ProviderSelectionFailureReason, SchedulingDecisionKind, SchedulingEvent,
        SchedulingEventEmitter, SchedulingStage, SelectionFailureReason,
    };
}

// Re-export shared types from legacy events module
pub use super::events::{CleanupReason, EventBuilder, EventMetadata, EventPublisher, TerminationReason, TraceContext};
