#![allow(ambiguous_glob_reexports)]
// Hodei Job Platform - Domain Layer
// Bounded Contexts reorganizados según DDD:
// - shared_kernel: Tipos base, IDs y errores compartidos
// - event_bus: Traits de mensajería
// - jobs: Job aggregate, JobSpec, JobQueue, templates y coordinación
// - workers: Worker aggregate, WorkerSpec, WorkerHandle, ProviderType
// - providers: ProviderConfig y configuración de providers
// - scheduling: Estrategias de scheduling y selección
// - audit: AuditLog y repositorio de auditoría
// - iam: Tokens OTP y autenticación

pub mod event_bus;
pub mod outbox;
pub mod shared_kernel;
pub mod telemetry;
pub mod testing;

// Bounded Contexts
pub mod audit;
pub mod command;
pub mod credentials;
pub mod domain_events;
pub mod iam;
pub mod jobs;
pub mod logging;
pub mod providers;
pub mod saga;
pub mod scheduling;
pub mod templates;
pub mod workers;

// Legacy module containing the original DomainEvent enum and related types
// This file contains the monolithic enum that we're gradually replacing
pub mod events {
    include!("events.rs");
}

// Re-exports for backward compatibility - these types are also available through domain_events
pub use events::{
    CleanupReason, DomainEvent, EventBuilder, EventMetadata, EventPublisher, TerminationReason,
    TraceContext,
};

// Public events module that provides both the legacy DomainEvent and new modular events
pub mod events_public {
    //! Domain Events Module
    //!
    //! This module provides access to both the legacy `DomainEvent` enum
    //! and the new modular event structures organized by bounded context.
    //!
    //! ## Usage
    //!
    //! ```rust
    //! // Legacy usage
    //! use hodei_domain::events_public::DomainEvent;
    //!
    //! // New modular usage
    //! use hodei_domain::events_public::jobs::JobCreated;
    //! ```

    pub use crate::domain_events::*;
    pub use crate::events::DomainEvent;
}

// Legacy exports para retrocompatibilidad durante la migración
pub mod request_context;

#[cfg(test)]
mod tests;

// Re-exports para facilitar el uso de los bounded contexts
pub use command::*;
pub use domain_events::*;
pub use logging::*;
pub use request_context::*;
pub use shared_kernel::*;

// Re-exports de bounded contexts
pub use audit::*;
pub use credentials::*;
pub use iam::*;
pub use jobs::*;
pub use providers::*;
pub use saga::*;
pub use scheduling::*;
pub use templates::*;
pub use workers::*;
