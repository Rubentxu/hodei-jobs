pub use hodei_shared::*;

pub mod error_values;

/// Errores del dominio
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: JobId },

    #[error("Provider not found: {provider_id}")]
    ProviderNotFound { provider_id: ProviderId },

    #[error("Invalid job state transition from {from_state} to {to_state} for job {job_id}")]
    InvalidStateTransition {
        job_id: JobId,
        from_state: JobState,
        to_state: JobState,
    },

    #[error("Invalid job spec field {field}: {reason}")]
    InvalidJobSpec { field: String, reason: String },

    #[error("Invalid worker spec field {field}: {reason}")]
    InvalidWorkerSpec { field: String, reason: String },

    #[error("Invalid max attempts value: {value} - {reason}")]
    InvalidMaxAttempts { value: u32, reason: String },

    #[error("Provider {provider_id} is not healthy")]
    ProviderUnhealthy { provider_id: ProviderId },

    #[error("Provider {provider_id} cannot execute job: {reason}")]
    ProviderCannotExecuteJob {
        provider_id: ProviderId,
        reason: String,
    },

    #[error("Job {job_id} has already been executed")]
    JobAlreadyExecuted { job_id: JobId },

    #[error("Job {job_id} has exceeded max attempts ({max_attempts})")]
    MaxAttemptsExceeded { job_id: JobId, max_attempts: u32 },

    #[error("Provider {provider_id} is overloaded")]
    ProviderOverloaded { provider_id: ProviderId },

    #[error("Invalid provider configuration: {message}")]
    InvalidProviderConfig { message: String },

    #[error("Job execution timeout: {job_id}")]
    JobExecutionTimeout { job_id: JobId },

    #[error("External service error: {service}: {message}")]
    ExternalServiceError { service: String, message: String },

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },

    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Worker {worker_id} is not available")]
    WorkerNotAvailable { worker_id: WorkerId },

    #[error("Worker provisioning failed: {message}")]
    WorkerProvisioningFailed { message: String },

    #[error("Worker provisioning timeout")]
    WorkerProvisioningTimeout,

    #[error("Worker recovery failed: {message}")]
    WorkerRecoveryFailed { message: String },

    #[error("No provider available for job requirements")]
    NoProviderAvailable,

    #[error("Worker {worker_id} already exists")]
    WorkerAlreadyExists { worker_id: WorkerId },

    #[error("Invalid worker state transition from {current} to {requested}")]
    InvalidWorkerStateTransition { current: String, requested: String },

    #[error("Invalid OTP token: {message}")]
    InvalidOtpToken { message: String },

    // EPIC-30: Saga-related errors
    #[error("Saga execution timeout after {duration:?}")]
    SagaTimeout { duration: std::time::Duration },

    #[error("Saga step '{step}' failed: {error}")]
    SagaStepFailed { step: String, error: String },

    #[error("Saga was compensated: {saga_id}")]
    SagaCompensated { saga_id: String },

    #[error("Saga error: {message}")]
    SagaError { message: String },

    #[error("Template parameter validation error: {message}")]
    TemplateParameterValidationError { message: String },

    #[error("Saga orchestrator error: {0}")]
    OrchestratorError(#[from] crate::saga::orchestrator::OrchestratorError),
}

impl From<sqlx::Error> for DomainError {
    fn from(error: sqlx::Error) -> Self {
        Self::InfrastructureError {
            message: format!("Database error: {}", error),
        }
    }
}

impl From<serde_json::Error> for DomainError {
    fn from(error: serde_json::Error) -> Self {
        Self::InfrastructureError {
            message: format!("Serialization error: {}", error),
        }
    }
}

pub type Result<T> = std::result::Result<T, DomainError>;

/// Trait para entidades con ID
pub trait Identifiable {
    type Id;
    fn id(&self) -> &Self::Id;
}

/// Trait para agregados
pub trait Aggregate {
    type Id;
    fn aggregate_id(&self) -> &Self::Id;
}

/// Trait para value objects
pub trait ValueObject {
    type Value;
    fn value(&self) -> &Self::Value;
}
