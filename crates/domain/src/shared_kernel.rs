// Shared Kernel - Tipos base y errores compartidos entre bounded contexts

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Identificador único para jobs
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identificador único para providers
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub Uuid);

impl ProviderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ProviderId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identificador único para workers
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerId(pub Uuid);

impl WorkerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    pub fn from_string(s: &str) -> Option<Self> {
        Uuid::parse_str(s).ok().map(Self)
    }
}

impl Default for WorkerId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for WorkerId {
    fn from(s: String) -> Self {
        Self::from_string(&s).unwrap_or_else(Self::new)
    }
}

/// Identificador de correlación para tracking de operaciones
#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(pub Uuid);

impl CorrelationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Estados posibles de un job (alineado con PRD v6.0)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobState {
    /// Job creado y en cola para ejecución
    Pending,
    /// Job asignado a un worker (scheduled)
    Scheduled,
    /// Job ejecutándose en el worker
    Running,
    /// Job completado exitosamente
    Succeeded,
    /// Job falló durante la ejecución
    Failed,
    /// Job cancelado por el usuario
    Cancelled,
    /// Job expiró por timeout
    Timeout,
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobState::Pending => write!(f, "PENDING"),
            JobState::Scheduled => write!(f, "SCHEDULED"),
            JobState::Running => write!(f, "RUNNING"),
            JobState::Succeeded => write!(f, "SUCCEEDED"),
            JobState::Failed => write!(f, "FAILED"),
            JobState::Cancelled => write!(f, "CANCELLED"),
            JobState::Timeout => write!(f, "TIMEOUT"),
        }
    }
}

/// Estados del ciclo de vida de un worker (alineado con PRD v6.0)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkerState {
    /// Worker solicitado, provider creándolo
    Creating,
    /// Worker creado, agent conectando vía gRPC
    Connecting,
    /// Agent registrado, listo para jobs
    Ready,
    /// Ejecutando un job
    Busy,
    /// No acepta nuevos jobs, pero termina el actual
    Draining,
    /// En proceso de terminación
    Terminating,
    /// Worker destruido
    Terminated,
}

impl fmt::Display for WorkerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerState::Creating => write!(f, "CREATING"),
            WorkerState::Connecting => write!(f, "CONNECTING"),
            WorkerState::Ready => write!(f, "READY"),
            WorkerState::Busy => write!(f, "BUSY"),
            WorkerState::Draining => write!(f, "DRAINING"),
            WorkerState::Terminating => write!(f, "TERMINATING"),
            WorkerState::Terminated => write!(f, "TERMINATED"),
        }
    }
}

impl WorkerState {
    /// Verifica si el worker puede aceptar jobs
    pub fn can_accept_jobs(&self) -> bool {
        matches!(self, WorkerState::Ready)
    }
    
    /// Verifica si el worker está activo
    pub fn is_active(&self) -> bool {
        matches!(self, 
            WorkerState::Creating | 
            WorkerState::Connecting | 
            WorkerState::Ready | 
            WorkerState::Busy |
            WorkerState::Draining
        )
    }
    
    /// Verifica si el worker ha terminado
    pub fn is_terminated(&self) -> bool {
        matches!(self, WorkerState::Terminated)
    }
    
    /// Verifica si el worker está en proceso de drenar (no acepta nuevos jobs)
    pub fn is_draining(&self) -> bool {
        matches!(self, WorkerState::Draining)
    }
}

/// Estados posibles de un provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderStatus {
    /// Provider disponible y saludable
    Active,
    /// Provider en mantenimiento temporal
    Maintenance,
    /// Provider deshabilitado
    Disabled,
    /// Provider sobrecargado
    Overloaded,
    /// Provider no saludable
    Unhealthy,
    /// Provider conectado pero con problemas
    Degraded,
}

impl fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderStatus::Active => write!(f, "ACTIVE"),
            ProviderStatus::Maintenance => write!(f, "MAINTENANCE"),
            ProviderStatus::Disabled => write!(f, "DISABLED"),
            ProviderStatus::Overloaded => write!(f, "OVERLOADED"),
            ProviderStatus::Unhealthy => write!(f, "UNHEALTHY"),
            ProviderStatus::Degraded => write!(f, "DEGRADED"),
        }
    }
}

/// Estados de ejecución específicos del provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Job enviado al provider
    Submitted,
    /// Job en cola del provider
    Queued,
    /// Job ejecutándose
    Running,
    /// Job completado exitosamente
    Succeeded,
    /// Job falló
    Failed,
    /// Job cancelado
    Cancelled,
    /// Job expiró
    TimedOut,
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionStatus::Submitted => write!(f, "SUBMITTED"),
            ExecutionStatus::Queued => write!(f, "QUEUED"),
            ExecutionStatus::Running => write!(f, "RUNNING"),
            ExecutionStatus::Succeeded => write!(f, "SUCCEEDED"),
            ExecutionStatus::Failed => write!(f, "FAILED"),
            ExecutionStatus::Cancelled => write!(f, "CANCELLED"),
            ExecutionStatus::TimedOut => write!(f, "TIMED_OUT"),
        }
    }
}

/// Resultado de ejecución de un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobResult {
    /// Job ejecutado exitosamente
    Success {
        exit_code: i32,
        output: String,
        error_output: String,
    },
    /// Job falló con error específico
    Failed {
        exit_code: i32,
        error_message: String,
        error_output: String,
    },
    /// Job cancelado
    Cancelled,
    /// Job expiró (timeout)
    Timeout,
}

impl fmt::Display for JobResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobResult::Success { exit_code, .. } => {
                write!(f, "SUCCESS (exit_code: {})", exit_code)
            }
            JobResult::Failed { exit_code, error_message, .. } => {
                write!(f, "FAILED (exit_code: {}): {}", exit_code, error_message)
            }
            JobResult::Cancelled => write!(f, "CANCELLED"),
            JobResult::Timeout => write!(f, "TIMEOUT"),
        }
    }
}

/// Errores del dominio
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: JobId },
    
    #[error("Provider not found: {provider_id}")]
    ProviderNotFound { provider_id: ProviderId },
    
    #[error("Invalid job state transition from {from} to {to}")]
    InvalidStateTransition { from: JobState, to: JobState },
    
    #[error("Provider {provider_id} is not healthy")]
    ProviderUnhealthy { provider_id: ProviderId },
    
    #[error("Provider {provider_id} cannot execute job: {reason}")]
    ProviderCannotExecuteJob { provider_id: ProviderId, reason: String },
    
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
    
    #[error("No provider available for job requirements")]
    NoProviderAvailable,
    
    #[error("Worker {worker_id} already exists")]
    WorkerAlreadyExists { worker_id: WorkerId },
    
    #[error("Invalid worker state transition from {current} to {requested}")]
    InvalidWorkerStateTransition { current: String, requested: String },

    #[error("Invalid OTP token: {message}")]
    InvalidOtpToken { message: String },
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
