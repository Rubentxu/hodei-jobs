// Hodei Job Platform - Domain Layer
// Bounded Contexts:
// - shared_kernel: Tipos base, IDs y errores compartidos
// - job_execution: Job aggregate, JobSpec, JobQueue
// - job_execution_coordination: ExecutionContext, coordination services
// - worker: Worker aggregate, WorkerSpec, WorkerHandle, ProviderType
// - worker_provider: WorkerProvider trait, ProviderCapabilities, JobRequirements
// - provider_config: ProviderConfig, ProviderTypeConfig (configuración de providers)

pub mod shared_kernel;
pub mod job_execution;
pub mod job_execution_coordination;
pub mod worker;
pub mod worker_provider;
pub mod worker_registry;
pub mod job_scheduler;
pub mod provider_config;
pub mod otp_token_store;
pub mod job_template;

#[cfg(test)]
mod tests;

pub use shared_kernel::*;
pub use job_execution::*;
pub use job_execution_coordination::*;
pub use worker::*;
pub use worker_provider::*;
pub use worker_registry::*;
pub use job_scheduler::*;
pub use provider_config::*;
pub use otp_token_store::*;
pub use job_template::*;