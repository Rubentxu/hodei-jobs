// Hodei Job Platform - Domain Layer
// Bounded Contexts:
// - shared_kernel: Tipos base, IDs y errores compartidos
// - job_execution: Job aggregate, JobSpec, JobQueue
// - job_execution_coordination: ExecutionContext, coordination services
// - worker: Worker aggregate, WorkerSpec, WorkerHandle, ProviderType
// - worker_provider: WorkerProvider trait, ProviderCapabilities, JobRequirements
// - provider_config: ProviderConfig, ProviderTypeConfig (configuración de providers)

pub mod audit;
pub mod event_bus;
pub mod events;
pub mod job_execution;
pub mod job_execution_coordination;
pub mod job_scheduler;
pub mod job_template;
pub mod otp_token_store;
pub mod provider_config;
pub mod request_context;
pub mod shared_kernel;
pub mod worker;
pub mod worker_provider;
pub mod worker_registry;

#[cfg(test)]
mod tests;

pub use event_bus::*;
pub use events::*;
pub use request_context::*;
pub use job_execution::*;
pub use job_execution_coordination::*;
pub use job_scheduler::*;
pub use job_template::*;
pub use otp_token_store::*;
pub use provider_config::*;
pub use shared_kernel::*;
pub use worker::*;
pub use worker_provider::*;
pub use worker_registry::*;
