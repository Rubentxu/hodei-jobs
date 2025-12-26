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

// Bounded Contexts
pub mod audit;
pub mod credentials;
pub mod iam;
pub mod jobs;
pub mod logging;
pub mod providers;
pub mod scheduling;
pub mod workers;

// Legacy exports para retrocompatibilidad durante la migración
pub mod events;
pub mod request_context;

#[cfg(test)]
mod tests;

// Re-exports para facilitar el uso de los bounded contexts
pub use event_bus::*;
pub use events::*;
pub use logging::*;
pub use request_context::*;
pub use shared_kernel::*;

// Re-exports de bounded contexts
pub use audit::*;
pub use credentials::*;
pub use iam::*;
pub use jobs::*;
pub use providers::*;
pub use scheduling::*;
pub use workers::*;
