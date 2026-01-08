use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Estados posibles de un job
///
/// # Estados de Intervención Manual
///
/// El estado `ManualInterventionRequired` fue añadido para manejar situaciones
/// donde un job no puede continuar automáticamente y requiere acción de un operador.
/// Esto es especialmente útil para:
/// - Timeouts donde el recovery automático no es posible
/// - Jobs huérfanos (worker perdido, sin modo de recovery)
/// - Errores de infraestructura que requieren atención
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobState {
    Pending,
    Assigned,
    Scheduled,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Timeout,
    /// El job requiere intervención manual para continuar
    /// Este estado permite que un operador analice y tome decisiones
    ManualInterventionRequired,
}

impl JobState {
    /// Valida si una transición de estado es válida según el State Machine del dominio
    ///
    /// Transiciones válidas según la máquina de estados:
    /// - Pending → Assigned, Scheduled, Failed, Cancelled, Timeout, ManualInterventionRequired
    /// - Assigned → Running, Failed, Timeout, Cancelled, ManualInterventionRequired
    /// - Scheduled → Running, Failed, Timeout, Cancelled, ManualInterventionRequired
    /// - Running → Succeeded, Failed, Cancelled, Timeout, ManualInterventionRequired
    /// - ManualInterventionRequired → Pending (después de intervención), Failed, Cancelled
    /// - Succeeded, Failed, Cancelled, Timeout → (terminal, no transiciones salientes)
    ///
    /// # Estados de Intervención Manual
    ///
    /// El estado `ManualInterventionRequired` permite:
    /// - Transicionar desde cualquier estado en progreso
    /// - Ser retomado a Pending (para reintento)
    /// - Ser completado manualmente como Failed o Cancelled
    pub fn can_transition_to(&self, new_state: &JobState) -> bool {
        match (self, new_state) {
            // Mismo estado - no es una transición válida
            (s, n) if s == n => false,

            // Transiciones válidas desde Pending
            (JobState::Pending, JobState::Assigned) => true,
            (JobState::Pending, JobState::Scheduled) => true,
            (JobState::Pending, JobState::Failed) => true,
            (JobState::Pending, JobState::Cancelled) => true,
            (JobState::Pending, JobState::Timeout) => true,
            (JobState::Pending, JobState::ManualInterventionRequired) => true,

            // Transiciones válidas desde Assigned (ALTA CONFIANZA)
            (JobState::Assigned, JobState::Running) => true,
            (JobState::Assigned, JobState::Failed) => true,
            (JobState::Assigned, JobState::Timeout) => true,
            (JobState::Assigned, JobState::Cancelled) => true,
            (JobState::Assigned, JobState::ManualInterventionRequired) => true,

            // Transiciones válidas desde Scheduled (CONFIANZA MEDIA)
            (JobState::Scheduled, JobState::Running) => true,
            (JobState::Scheduled, JobState::Failed) => true,
            (JobState::Scheduled, JobState::Timeout) => true,
            (JobState::Scheduled, JobState::Cancelled) => true,
            (JobState::Scheduled, JobState::ManualInterventionRequired) => true,

            // Transiciones válidas desde Running
            (JobState::Running, JobState::Succeeded) => true,
            (JobState::Running, JobState::Failed) => true,
            (JobState::Running, JobState::Cancelled) => true,
            (JobState::Running, JobState::Timeout) => true,
            (JobState::Running, JobState::ManualInterventionRequired) => true,

            // Transiciones válidas desde ManualInterventionRequired
            (JobState::ManualInterventionRequired, JobState::Pending) => true,
            (JobState::ManualInterventionRequired, JobState::Failed) => true,
            (JobState::ManualInterventionRequired, JobState::Cancelled) => true,

            // Todas las demás transiciones son inválidas
            _ => false,
        }
    }

    /// Retorna true si el estado es terminal (no se puede continuar)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobState::Succeeded
                | JobState::Failed
                | JobState::Cancelled
                | JobState::Timeout
        )
        // ManualInterventionRequired NO es terminal - puede volver a Pending
    }

    /// Retorna true si el estado está en progreso
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            JobState::Pending
                | JobState::Assigned
                | JobState::Scheduled
                | JobState::Running
                | JobState::ManualInterventionRequired
        )
    }

    /// Retorna true si el job requiere intervención manual
    pub fn requires_manual_intervention(&self) -> bool {
        matches!(self, JobState::ManualInterventionRequired)
    }

    /// Retorna true si el job está listo para ejecutar (Assigned o Scheduled)
    /// Indica que el scheduler ya tomó una decisión
    pub fn is_scheduled(&self) -> bool {
        matches!(self, JobState::Assigned | JobState::Scheduled)
    }

    /// Retorna la categoría simplificada del estado para reporting
    pub fn category(&self) -> &'static str {
        match self {
            JobState::Pending => "pending",
            JobState::Assigned => "assigned",
            JobState::Scheduled => "scheduled",
            JobState::Running => "running",
            JobState::Succeeded => "completed",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
            JobState::Timeout => "timeout",
            JobState::ManualInterventionRequired => "manual_intervention",
        }
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobState::Pending => write!(f, "PENDING"),
            JobState::Assigned => write!(f, "ASSIGNED"),
            JobState::Scheduled => write!(f, "SCHEDULED"),
            JobState::Running => write!(f, "RUNNING"),
            JobState::Succeeded => write!(f, "SUCCEEDED"),
            JobState::Failed => write!(f, "FAILED"),
            JobState::Cancelled => write!(f, "CANCELLED"),
            JobState::Timeout => write!(f, "TIMEOUT"),
            JobState::ManualInterventionRequired => write!(f, "MANUAL_INTERVENTION_REQUIRED"),
        }
    }
}

impl FromStr for JobState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PENDING" => Ok(JobState::Pending),
            "ASSIGNED" => Ok(JobState::Assigned),
            "SCHEDULED" => Ok(JobState::Scheduled),
            "RUNNING" => Ok(JobState::Running),
            "SUCCEEDED" => Ok(JobState::Succeeded),
            "FAILED" => Ok(JobState::Failed),
            "CANCELLED" => Ok(JobState::Cancelled),
            "TIMEOUT" => Ok(JobState::Timeout),
            "MANUAL_INTERVENTION_REQUIRED" => Ok(JobState::ManualInterventionRequired),
            _ => Err(format!("Invalid JobState: {}", s)),
        }
    }
}

impl TryFrom<i32> for JobState {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(JobState::Pending),
            1 => Ok(JobState::Assigned),
            2 => Ok(JobState::Scheduled),
            3 => Ok(JobState::Running),
            4 => Ok(JobState::Succeeded),
            5 => Ok(JobState::Failed),
            6 => Ok(JobState::Cancelled),
            7 => Ok(JobState::Timeout),
            8 => Ok(JobState::ManualInterventionRequired),
            _ => Err(format!("Invalid JobState value: {}", value)),
        }
    }
}

impl From<&JobState> for i32 {
    fn from(state: &JobState) -> Self {
        match state {
            JobState::Pending => 0,
            JobState::Assigned => 1,
            JobState::Scheduled => 2,
            JobState::Running => 3,
            JobState::Succeeded => 4,
            JobState::Failed => 5,
            JobState::Cancelled => 6,
            JobState::Timeout => 7,
            JobState::ManualInterventionRequired => 8,
        }
    }
}

/// Estados del ciclo de vida de un worker (Crash-Only Design)
///
/// Estados simplificados según EPIC-43 Sprint 3:
/// - Creating: Worker provisionándose, agent iniciándose
/// - Ready: Worker listo para recibir jobs
/// - Busy: Worker ejecutando un job
/// - Terminated: Worker destruido (estado terminal)
///
/// Los estados transitorios (Connecting, Draining, Terminating) fueron eliminados.
/// En su lugar, los errores durante Creating van directamente a Terminated.
/// La terminación es inmediata (no graceful drain para workers efímeros).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkerState {
    Creating,
    Ready,
    Busy,
    Terminated,
}

impl fmt::Display for WorkerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerState::Creating => write!(f, "CREATING"),
            WorkerState::Ready => write!(f, "READY"),
            WorkerState::Busy => write!(f, "BUSY"),
            WorkerState::Terminated => write!(f, "TERMINATED"),
        }
    }
}

impl FromStr for WorkerState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CREATING" => Ok(WorkerState::Creating),
            "READY" => Ok(WorkerState::Ready),
            "BUSY" => Ok(WorkerState::Busy),
            "TERMINATED" => Ok(WorkerState::Terminated),
            _ => Err(format!("Invalid WorkerState: {}", s)),
        }
    }
}

impl TryFrom<i32> for WorkerState {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WorkerState::Creating),
            1 => Ok(WorkerState::Ready),
            2 => Ok(WorkerState::Busy),
            3 => Ok(WorkerState::Terminated),
            _ => Err(format!("Invalid WorkerState value: {}", value)),
        }
    }
}

impl From<&WorkerState> for i32 {
    fn from(state: &WorkerState) -> Self {
        match state {
            WorkerState::Creating => 0,
            WorkerState::Ready => 1,
            WorkerState::Busy => 2,
            WorkerState::Terminated => 3,
        }
    }
}

impl WorkerState {
    /// Valida si una transición de estado es válida según el State Machine del dominio.
    ///
    /// Transiciones válidas según Crash-Only Design (4 estados):
    /// - Creating → Ready (éxito), Terminated (error)
    /// - Ready → Busy (job asignado), Terminated (terminación directa)
    /// - Busy → Terminated (job completado o error)
    /// - Terminated → (terminal, no transiciones salientes)
    ///
    /// Los estados intermedios (Connecting, Draining, Terminating) fueron eliminados.
    /// Esto simplifica el modelo y elimina condiciones de carrera.
    ///
    /// # Ejemplo
    ///
    /// ```rust
    /// use hodei_shared::states::WorkerState;
    ///
    /// assert!(WorkerState::Creating.can_transition_to(&WorkerState::Ready));
    /// assert!(WorkerState::Creating.can_transition_to(&WorkerState::Terminated)); // Error
    /// assert!(!WorkerState::Terminated.can_transition_to(&WorkerState::Ready));
    /// ```
    pub fn can_transition_to(&self, new_state: &WorkerState) -> bool {
        match (self, new_state) {
            // Mismo estado - no es una transición válida
            (s, n) if s == n => false,

            // Transiciones válidas desde Creating
            (WorkerState::Creating, WorkerState::Ready) => true,
            (WorkerState::Creating, WorkerState::Terminated) => true, // Error durante creación

            // Transiciones válidas desde Ready
            (WorkerState::Ready, WorkerState::Busy) => true,
            (WorkerState::Ready, WorkerState::Terminated) => true, // Terminación directa

            // Transiciones válidas desde Busy
            (WorkerState::Busy, WorkerState::Terminated) => true, // Job completado o error

            // Todas las demás transiciones son inválidas
            _ => false,
        }
    }

    /// Retorna true si el estado es terminal (no se puede continuar)
    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkerState::Terminated)
    }

    /// Retorna true si el estado está en progreso (no es terminal)
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            WorkerState::Creating | WorkerState::Ready | WorkerState::Busy
        )
    }

    /// Retorna true si el estado es de terminación o terminado
    pub fn is_terminating(&self) -> bool {
        matches!(self, WorkerState::Terminated)
    }

    /// Retorna true si el worker puede aceptar jobs
    pub fn can_accept_jobs(&self) -> bool {
        matches!(self, WorkerState::Ready)
    }

    /// Retorna true si el worker está activo (no terminado)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            WorkerState::Creating | WorkerState::Ready | WorkerState::Busy
        )
    }

    /// Retorna true si el worker está terminado
    pub fn is_terminated(&self) -> bool {
        matches!(self, WorkerState::Terminated)
    }

    /// Retorna true si el worker está listo para recibir jobs
    pub fn is_ready(&self) -> bool {
        matches!(self, WorkerState::Ready)
    }
}

/// Estados posibles de un provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderStatus {
    Active,
    Maintenance,
    Disabled,
    Overloaded,
    Unhealthy,
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

impl FromStr for ProviderStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ACTIVE" => Ok(ProviderStatus::Active),
            "MAINTENANCE" => Ok(ProviderStatus::Maintenance),
            "DISABLED" => Ok(ProviderStatus::Disabled),
            "OVERLOADED" => Ok(ProviderStatus::Overloaded),
            "UNHEALTHY" => Ok(ProviderStatus::Unhealthy),
            "DEGRADED" => Ok(ProviderStatus::Degraded),
            _ => Err(format!("Invalid ProviderStatus: {}", s)),
        }
    }
}

impl TryFrom<i32> for ProviderStatus {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProviderStatus::Active),
            1 => Ok(ProviderStatus::Maintenance),
            2 => Ok(ProviderStatus::Disabled),
            3 => Ok(ProviderStatus::Overloaded),
            4 => Ok(ProviderStatus::Unhealthy),
            5 => Ok(ProviderStatus::Degraded),
            _ => Err(format!("Invalid ProviderStatus value: {}", value)),
        }
    }
}

impl From<&ProviderStatus> for i32 {
    fn from(status: &ProviderStatus) -> Self {
        match status {
            ProviderStatus::Active => 0,
            ProviderStatus::Maintenance => 1,
            ProviderStatus::Disabled => 2,
            ProviderStatus::Overloaded => 3,
            ProviderStatus::Unhealthy => 4,
            ProviderStatus::Degraded => 5,
        }
    }
}

/// Estados de ejecución específicos del provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Submitted,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
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

impl FromStr for ExecutionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SUBMITTED" => Ok(ExecutionStatus::Submitted),
            "QUEUED" => Ok(ExecutionStatus::Queued),
            "RUNNING" => Ok(ExecutionStatus::Running),
            "SUCCEEDED" => Ok(ExecutionStatus::Succeeded),
            "FAILED" => Ok(ExecutionStatus::Failed),
            "CANCELLED" => Ok(ExecutionStatus::Cancelled),
            "TIMED_OUT" => Ok(ExecutionStatus::TimedOut),
            _ => Err(format!("Invalid ExecutionStatus: {}", s)),
        }
    }
}

impl TryFrom<i32> for ExecutionStatus {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ExecutionStatus::Submitted),
            1 => Ok(ExecutionStatus::Queued),
            2 => Ok(ExecutionStatus::Running),
            3 => Ok(ExecutionStatus::Succeeded),
            4 => Ok(ExecutionStatus::Failed),
            5 => Ok(ExecutionStatus::Cancelled),
            6 => Ok(ExecutionStatus::TimedOut),
            _ => Err(format!("Invalid ExecutionStatus value: {}", value)),
        }
    }
}

impl From<&ExecutionStatus> for i32 {
    fn from(status: &ExecutionStatus) -> Self {
        match status {
            ExecutionStatus::Submitted => 0,
            ExecutionStatus::Queued => 1,
            ExecutionStatus::Running => 2,
            ExecutionStatus::Succeeded => 3,
            ExecutionStatus::Failed => 4,
            ExecutionStatus::Cancelled => 5,
            ExecutionStatus::TimedOut => 6,
        }
    }
}

/// Resultado de ejecución de un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobResult {
    Success {
        exit_code: i32,
        output: String,
        error_output: String,
    },
    Failed {
        exit_code: i32,
        error_message: String,
        error_output: String,
    },
    Cancelled,
    Timeout,
}

impl fmt::Display for JobResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobResult::Success { exit_code, .. } => {
                write!(f, "SUCCESS (exit_code: {})", exit_code)
            }
            JobResult::Failed {
                exit_code,
                error_message,
                ..
            } => {
                write!(f, "FAILED (exit_code: {}): {}", exit_code, error_message)
            }
            JobResult::Cancelled => write!(f, "CANCELLED"),
            JobResult::Timeout => write!(f, "TIMEOUT"),
        }
    }
}

/// Categorías de fallo para jobs execution errors.
///
/// Este enum proporciona categorización estructurada de errores de ejecución,
/// permitiendo a los clientes entender rápidamente qué falló y cómo resolverlo.
/// Reduce Connascence of Position transformando mensajes de error string
/// en tipos estructurados con contexto.
///
/// # Ejemplo de uso
///
/// ```
/// use hodei_shared::states::JobFailureReason;
///
/// let reason = JobFailureReason::PermissionDenied {
///     path: "/app/scripts/run.sh".to_string(),
///     operation: "execute".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobFailureReason {
    /// El comando no fue encontrado en el PATH
    CommandNotFound {
        /// El comando que no se encontró
        command: String,
    },
    /// Permiso denegado al acceder a un recurso
    PermissionDenied {
        /// Path del recurso al que no se pudo acceder
        path: String,
        /// Operación que se intentó (execute, read, write)
        operation: String,
    },
    /// Archivo o directorio no encontrado
    FileNotFound {
        /// Path que no existe
        path: String,
    },
    /// Error al hacer spawn del proceso
    ProcessSpawnFailed {
        /// Mensaje descriptivo del error
        message: String,
    },
    /// Timeout de ejecución excedido
    ExecutionTimeout {
        /// Límite de timeout en segundos
        limit_secs: u64,
    },
    /// El proceso recibió una señal (SIGTERM, SIGKILL, etc.)
    SignalReceived {
        /// Número de la señal recibida
        signal: i32,
    },
    /// El proceso exited con código de salida no cero (error de aplicación)
    NonZeroExitCode {
        /// Código de salida del proceso
        exit_code: i32,
    },
    /// Error de infraestructura (conexión perdida, provider error, etc.)
    InfrastructureError {
        /// Mensaje descriptivo del error de infraestructura
        message: String,
    },
    /// Error de validación de los parámetros del job
    ValidationError {
        /// Campo que falló la validación
        field: String,
        /// Razón de la validación fallida
        reason: String,
    },
    /// Error de configuración del job
    ConfigurationError {
        /// Clave de configuración que falló
        key: String,
        /// Mensaje descriptivo
        message: String,
    },
    /// Error de secret injection
    SecretInjectionError {
        /// Nombre del secret que falló
        secret_name: String,
        /// Mensaje descriptivo
        message: String,
    },
    /// Error de I/O durante la ejecución
    IoError {
        /// Operación de I/O que falló
        operation: String,
        /// Path involucrado
        path: Option<String>,
        /// Mensaje del sistema
        message: String,
    },
    /// Error desconocido/no categorizado (fallback)
    Unknown {
        /// Mensaje de error original
        message: String,
    },
}

impl fmt::Display for JobFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobFailureReason::CommandNotFound { command } => {
                write!(f, "COMMAND_NOT_FOUND: {}", command)
            }
            JobFailureReason::PermissionDenied { path, operation } => {
                write!(f, "PERMISSION_DENIED: {} on {}", operation, path)
            }
            JobFailureReason::FileNotFound { path } => {
                write!(f, "FILE_NOT_FOUND: {}", path)
            }
            JobFailureReason::ProcessSpawnFailed { message } => {
                write!(f, "PROCESS_SPAWN_FAILED: {}", message)
            }
            JobFailureReason::ExecutionTimeout { limit_secs } => {
                write!(f, "EXECUTION_TIMEOUT: exceeded {} seconds", limit_secs)
            }
            JobFailureReason::SignalReceived { signal } => {
                write!(f, "SIGNAL_RECEIVED: signal {}", signal)
            }
            JobFailureReason::NonZeroExitCode { exit_code } => {
                write!(f, "NON_ZERO_EXIT_CODE: exit code {}", exit_code)
            }
            JobFailureReason::InfrastructureError { message } => {
                write!(f, "INFRASTRUCTURE_ERROR: {}", message)
            }
            JobFailureReason::ValidationError { field, reason } => {
                write!(f, "VALIDATION_ERROR: {} - {}", field, reason)
            }
            JobFailureReason::ConfigurationError { key, message } => {
                write!(f, "CONFIGURATION_ERROR: {} - {}", key, message)
            }
            JobFailureReason::SecretInjectionError {
                secret_name,
                message,
            } => {
                write!(f, "SECRET_INJECTION_ERROR: {} - {}", secret_name, message)
            }
            JobFailureReason::IoError {
                operation,
                path,
                message,
            } => {
                if let Some(p) = path {
                    write!(f, "IO_ERROR: {} on {} - {}", operation, p, message)
                } else {
                    write!(f, "IO_ERROR: {} - {}", operation, message)
                }
            }
            JobFailureReason::Unknown { message } => {
                write!(f, "UNKNOWN_ERROR: {}", message)
            }
        }
    }
}

impl JobFailureReason {
    /// Genera acciones sugeridas para resolver el error.
    ///
    /// Cada tipo de error tiene acciones específicas que el usuario
    /// puede tomar para resolver el problema.
    pub fn suggested_actions(&self) -> Vec<String> {
        match self {
            JobFailureReason::CommandNotFound { command } => vec![
                format!("Verify that '{}' is installed and in PATH", command),
                "Install the required package or dependency".to_string(),
                "Use the full path to the command instead".to_string(),
            ],
            JobFailureReason::PermissionDenied { path, operation } => vec![
                format!("Check permissions for: {}", path),
                format!(
                    "Run: chmod +{} {}",
                    operation
                        .chars()
                        .next()
                        .map(|c| match c {
                            'e' => 'x',
                            'r' => 'r',
                            'w' => 'w',
                            _ => 'x',
                        })
                        .unwrap_or('x'),
                    path
                ),
                "Verify the user running the job has appropriate permissions".to_string(),
            ],
            JobFailureReason::FileNotFound { path } => vec![
                format!("Verify the file exists: {}", path),
                "Check the working directory configuration".to_string(),
                "Create the missing file or fix the path".to_string(),
            ],
            JobFailureReason::ProcessSpawnFailed { .. } => vec![
                "Check system resources (memory, file descriptors)".to_string(),
                "Review the command syntax and arguments".to_string(),
                "Check for conflicting processes".to_string(),
            ],
            JobFailureReason::ExecutionTimeout { limit_secs } => vec![
                format!("Increase timeout limit (current: {}s)", limit_secs),
                "Optimize the job script to run faster".to_string(),
                "Break the job into smaller chunks".to_string(),
            ],
            JobFailureReason::SignalReceived { signal } => match signal {
                9 | 15 => vec![
                    "The process was terminated (SIGTERM/SIGKILL)".to_string(),
                    "Check if there's a timeout or resource limit configured".to_string(),
                    "Review external systems that might be killing the process".to_string(),
                ],
                11 => vec![
                    "Segmentation fault - the program crashed".to_string(),
                    "Check for memory issues in the application".to_string(),
                    "Verify the application is compatible with the environment".to_string(),
                ],
                _ => vec![
                    format!("Process received signal: {}", signal),
                    "Check system logs for more details".to_string(),
                    "Review the application for potential crashes".to_string(),
                ],
            },
            JobFailureReason::NonZeroExitCode { .. } => vec![
                "Check the application logs for specific error messages".to_string(),
                "Verify all required inputs and configurations are correct".to_string(),
                "Review the application documentation for exit code meanings".to_string(),
            ],
            JobFailureReason::InfrastructureError { .. } => vec![
                "Check the provider status and connectivity".to_string(),
                "Verify resource availability (CPU, memory, disk)".to_string(),
                "Contact support if the issue persists".to_string(),
            ],
            JobFailureReason::ValidationError { field, reason } => vec![
                format!("Fix the '{}' field: {}", field, reason),
                "Review the job specification for required fields".to_string(),
                "Update the configuration with valid values".to_string(),
            ],
            JobFailureReason::ConfigurationError { key, message } => vec![
                format!("Fix configuration key '{}': {}", key, message),
                "Check environment variables and configuration files".to_string(),
                "Verify all required secrets and keys are set".to_string(),
            ],
            JobFailureReason::SecretInjectionError {
                secret_name,
                message,
            } => vec![
                format!("Verify secret '{}' exists and is accessible", secret_name),
                format!("Secret injection error: {}", message),
                "Check IAM/permissions for secret access".to_string(),
            ],
            JobFailureReason::IoError {
                operation,
                path: _,
                message: _,
            } => vec![
                format!("Retry the '{}' operation", operation),
                "Check disk space and file system health".to_string(),
                "Verify the path is accessible and not corrupted".to_string(),
            ],
            JobFailureReason::Unknown { .. } => vec![
                "Review the full error message and logs".to_string(),
                "Try running the job with more verbose logging".to_string(),
                "Contact support with the error details".to_string(),
            ],
        }
    }

    /// Returns true if this is an infrastructure error (not user's fault)
    pub fn is_infrastructure_error(&self) -> bool {
        matches!(
            self,
            JobFailureReason::InfrastructureError { .. }
                | JobFailureReason::ProcessSpawnFailed { .. }
                | JobFailureReason::IoError { .. }
        )
    }

    /// Returns true if this is a user error (configuration, permissions, etc.)
    pub fn is_user_error(&self) -> bool {
        matches!(
            self,
            JobFailureReason::CommandNotFound { .. }
                | JobFailureReason::PermissionDenied { .. }
                | JobFailureReason::FileNotFound { .. }
                | JobFailureReason::ValidationError { .. }
                | JobFailureReason::ConfigurationError { .. }
                | JobFailureReason::SecretInjectionError { .. }
                | JobFailureReason::ExecutionTimeout { .. }
        )
    }
}

/// Razón de fallo para dispatch de job al worker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DispatchFailureReason {
    /// Worker no estaba en estado Ready para recibir jobs
    WorkerNotReady {
        /// Estado actual del worker
        current_state: String,
    },
    /// Timeout esperando respuesta del worker
    CommunicationTimeout {
        /// Tiempo esperado en milisegundos
        timeout_ms: u64,
    },
    /// Worker crasheó durante el dispatch
    WorkerCrashed,
    /// El canal de comunicación se cerró inesperadamente
    ChannelClosed,
    /// Error de protocolo (mensaje malformado, etc.)
    ProtocolError {
        /// Mensaje descriptivo
        message: String,
    },
    /// Error de red
    NetworkError {
        /// Mensaje descriptivo
        message: String,
    },
}

impl fmt::Display for DispatchFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispatchFailureReason::WorkerNotReady { current_state } => {
                write!(f, "WORKER_NOT_READY: worker in state {}", current_state)
            }
            DispatchFailureReason::CommunicationTimeout { timeout_ms } => {
                write!(f, "COMMUNICATION_TIMEOUT: exceeded {}ms", timeout_ms)
            }
            DispatchFailureReason::WorkerCrashed => {
                write!(f, "WORKER_CRASHED")
            }
            DispatchFailureReason::ChannelClosed => {
                write!(f, "CHANNEL_CLOSED")
            }
            DispatchFailureReason::ProtocolError { message } => {
                write!(f, "PROTOCOL_ERROR: {}", message)
            }
            DispatchFailureReason::NetworkError { message } => {
                write!(f, "NETWORK_ERROR: {}", message)
            }
        }
    }
}

/// Razón de fallo para provisioning de worker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProvisioningFailureReason {
    /// Fallo al descargar la imagen del worker
    ImagePullFailed {
        /// Nombre de la imagen
        image: String,
        /// Mensaje de error
        message: String,
    },
    /// Fallo al asignar recursos (CPU, memoria, etc.)
    ResourceAllocationFailed {
        /// Recurso que no se pudo asignar
        resource: String,
        /// Mensaje de error
        message: String,
    },
    /// Fallo al configurar la red del worker
    NetworkSetupFailed {
        /// Mensaje de error
        message: String,
    },
    /// Fallo de autenticación con el provider
    AuthenticationFailed {
        /// Mensaje de error
        message: String,
    },
    /// Timeout durante el provisioning
    Timeout,
    /// Provider no disponible
    ProviderUnavailable,
    /// Error interno del provider
    InternalError {
        /// Mensaje de error
        message: String,
    },
    /// Configuración inválida del worker
    InvalidConfiguration {
        /// Campo de configuración inválido
        field: String,
        /// Razón
        reason: String,
    },
}

impl fmt::Display for ProvisioningFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProvisioningFailureReason::ImagePullFailed { image, message } => {
                write!(f, "IMAGE_PULL_FAILED: {} - {}", image, message)
            }
            ProvisioningFailureReason::ResourceAllocationFailed { resource, message } => {
                write!(f, "RESOURCE_ALLOCATION_FAILED: {} - {}", resource, message)
            }
            ProvisioningFailureReason::NetworkSetupFailed { message } => {
                write!(f, "NETWORK_SETUP_FAILED: {}", message)
            }
            ProvisioningFailureReason::AuthenticationFailed { message } => {
                write!(f, "AUTHENTICATION_FAILED: {}", message)
            }
            ProvisioningFailureReason::Timeout => {
                write!(f, "PROVISIONING_TIMEOUT")
            }
            ProvisioningFailureReason::ProviderUnavailable => {
                write!(f, "PROVIDER_UNAVAILABLE")
            }
            ProvisioningFailureReason::InternalError { message } => {
                write!(f, "INTERNAL_ERROR: {}", message)
            }
            ProvisioningFailureReason::InvalidConfiguration { field, reason } => {
                write!(f, "INVALID_CONFIGURATION: {} - {}", field, reason)
            }
        }
    }
}

/// Razón de fallo para decisión de scheduling
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchedulingFailureReason {
    /// No hay providers disponibles en el sistema
    NoProvidersAvailable,
    /// Ningún provider coincide con los requisitos del job
    NoMatchingProviders {
        /// Requisito que no se pudo cumplir
        requirement: String,
    },
    /// Todos los providers están sobrecargados
    ProviderOverloaded,
    /// Provider no está saludable
    ProviderUnhealthy {
        /// ID del provider
        provider_id: String,
    },
    /// No se cumplieron las restricciones de recursos
    ResourceConstraintsNotMet {
        /// Recurso que falta
        resource: String,
        /// Cantidad requerida
        required: String,
        /// Cantidad disponible
        available: String,
    },
    /// Error interno del scheduler
    InternalError {
        /// Mensaje de error
        message: String,
    },
    /// Job fue cancelado antes de que el scheduler tomara decisión
    JobCancelled,
    /// Job expiró en la cola
    JobExpired,
}

impl fmt::Display for SchedulingFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchedulingFailureReason::NoProvidersAvailable => {
                write!(f, "NO_PROVIDERS_AVAILABLE")
            }
            SchedulingFailureReason::NoMatchingProviders { requirement } => {
                write!(f, "NO_MATCHING_PROVIDERS: {}", requirement)
            }
            SchedulingFailureReason::ProviderOverloaded => {
                write!(f, "PROVIDER_OVERLOADED")
            }
            SchedulingFailureReason::ProviderUnhealthy { provider_id } => {
                write!(f, "PROVIDER_UNHEALTHY: {}", provider_id)
            }
            SchedulingFailureReason::ResourceConstraintsNotMet {
                resource,
                required,
                available,
            } => {
                write!(
                    f,
                    "RESOURCE_CONSTRAINTS_NOT_MET: {} required={}, available={}",
                    resource, required, available
                )
            }
            SchedulingFailureReason::InternalError { message } => {
                write!(f, "INTERNAL_ERROR: {}", message)
            }
            SchedulingFailureReason::JobCancelled => {
                write!(f, "JOB_CANCELLED")
            }
            SchedulingFailureReason::JobExpired => {
                write!(f, "JOB_EXPIRED")
            }
        }
    }
}

/// Tipo de error de provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderErrorType {
    ConnectionLost,
    AuthenticationFailed,
    ResourceLimitExceeded,
    WorkerNotFound,
    ProvisioningFailed,
    Timeout,
    ConfigurationError,
    Internal,
}

impl fmt::Display for ProviderErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderErrorType::ConnectionLost => write!(f, "CONNECTION_LOST"),
            ProviderErrorType::AuthenticationFailed => write!(f, "AUTHENTICATION_FAILED"),
            ProviderErrorType::ResourceLimitExceeded => write!(f, "RESOURCE_LIMIT_EXCEEDED"),
            ProviderErrorType::WorkerNotFound => write!(f, "WORKER_NOT_FOUND"),
            ProviderErrorType::ProvisioningFailed => write!(f, "PROVISIONING_FAILED"),
            ProviderErrorType::Timeout => write!(f, "TIMEOUT"),
            ProviderErrorType::ConfigurationError => write!(f, "CONFIGURATION_ERROR"),
            ProviderErrorType::Internal => write!(f, "INTERNAL_ERROR"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // JobState tests
    #[test]
    fn test_job_state_from_str() {
        assert_eq!("PENDING".parse::<JobState>().unwrap(), JobState::Pending);
        assert_eq!("ASSIGNED".parse::<JobState>().unwrap(), JobState::Assigned);
        assert_eq!(
            "SCHEDULED".parse::<JobState>().unwrap(),
            JobState::Scheduled
        );
        assert_eq!("RUNNING".parse::<JobState>().unwrap(), JobState::Running);
        assert_eq!(
            "SUCCEEDED".parse::<JobState>().unwrap(),
            JobState::Succeeded
        );
        assert_eq!("FAILED".parse::<JobState>().unwrap(), JobState::Failed);
        assert_eq!(
            "CANCELLED".parse::<JobState>().unwrap(),
            JobState::Cancelled
        );
        assert_eq!("TIMEOUT".parse::<JobState>().unwrap(), JobState::Timeout);

        assert!("INVALID".parse::<JobState>().is_err());
    }

    #[test]
    fn test_job_state_try_from_i32() {
        assert_eq!(JobState::try_from(0).unwrap(), JobState::Pending);
        assert_eq!(JobState::try_from(1).unwrap(), JobState::Assigned);
        assert_eq!(JobState::try_from(2).unwrap(), JobState::Scheduled);
        assert_eq!(JobState::try_from(3).unwrap(), JobState::Running);
        assert_eq!(JobState::try_from(4).unwrap(), JobState::Succeeded);
        assert_eq!(JobState::try_from(5).unwrap(), JobState::Failed);
        assert_eq!(JobState::try_from(6).unwrap(), JobState::Cancelled);
        assert_eq!(JobState::try_from(7).unwrap(), JobState::Timeout);

        assert!(JobState::try_from(99).is_err());
    }

    #[test]
    fn test_job_state_into_i32() {
        assert_eq!(i32::from(&JobState::Pending), 0);
        assert_eq!(i32::from(&JobState::Assigned), 1);
        assert_eq!(i32::from(&JobState::Scheduled), 2);
        assert_eq!(i32::from(&JobState::Running), 3);
        assert_eq!(i32::from(&JobState::Succeeded), 4);
        assert_eq!(i32::from(&JobState::Failed), 5);
        assert_eq!(i32::from(&JobState::Cancelled), 6);
        assert_eq!(i32::from(&JobState::Timeout), 7);
    }

    #[test]
    fn test_job_state_transitions() {
        // Valid transitions from Pending
        assert!(JobState::Pending.can_transition_to(&JobState::Assigned));
        assert!(JobState::Pending.can_transition_to(&JobState::Scheduled));
        assert!(JobState::Pending.can_transition_to(&JobState::Failed));
        assert!(JobState::Pending.can_transition_to(&JobState::Cancelled));
        assert!(JobState::Pending.can_transition_to(&JobState::Timeout));

        // Valid transitions from Assigned
        assert!(JobState::Assigned.can_transition_to(&JobState::Running));
        assert!(JobState::Assigned.can_transition_to(&JobState::Failed));
        assert!(JobState::Assigned.can_transition_to(&JobState::Cancelled));
        assert!(JobState::Assigned.can_transition_to(&JobState::Timeout));

        // Valid transitions from Scheduled
        assert!(JobState::Scheduled.can_transition_to(&JobState::Running));
        assert!(JobState::Scheduled.can_transition_to(&JobState::Failed));
        assert!(JobState::Scheduled.can_transition_to(&JobState::Cancelled));
        assert!(JobState::Scheduled.can_transition_to(&JobState::Timeout));

        // Valid transitions from Running
        assert!(JobState::Running.can_transition_to(&JobState::Succeeded));
        assert!(JobState::Running.can_transition_to(&JobState::Failed));
        assert!(JobState::Running.can_transition_to(&JobState::Cancelled));
        assert!(JobState::Running.can_transition_to(&JobState::Timeout));

        // Invalid transitions
        assert!(!JobState::Pending.can_transition_to(&JobState::Running)); // Skip intermediate
        assert!(!JobState::Running.can_transition_to(&JobState::Pending)); // No backwards
        assert!(!JobState::Succeeded.can_transition_to(&JobState::Running)); // Terminal
    }

    #[test]
    fn test_job_state_terminal() {
        assert!(!JobState::Pending.is_terminal());
        assert!(!JobState::Assigned.is_terminal());
        assert!(!JobState::Scheduled.is_terminal());
        assert!(!JobState::Running.is_terminal());
        assert!(JobState::Succeeded.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
        assert!(JobState::Timeout.is_terminal());
    }

    #[test]
    fn test_job_state_in_progress() {
        assert!(JobState::Pending.is_in_progress());
        assert!(JobState::Assigned.is_in_progress());
        assert!(JobState::Scheduled.is_in_progress());
        assert!(JobState::Running.is_in_progress());
        assert!(!JobState::Succeeded.is_in_progress());
        assert!(!JobState::Failed.is_in_progress());
        assert!(!JobState::Cancelled.is_in_progress());
        assert!(!JobState::Timeout.is_in_progress());
    }

    #[test]
    fn test_job_state_is_scheduled() {
        assert!(!JobState::Pending.is_scheduled());
        assert!(JobState::Assigned.is_scheduled());
        assert!(JobState::Scheduled.is_scheduled());
        assert!(!JobState::Running.is_scheduled());
        assert!(!JobState::Succeeded.is_scheduled());
        assert!(!JobState::Failed.is_scheduled());
        assert!(!JobState::Cancelled.is_scheduled());
        assert!(!JobState::Timeout.is_scheduled());
    }

    #[test]
    fn test_job_state_category() {
        assert_eq!(JobState::Pending.category(), "pending");
        assert_eq!(JobState::Assigned.category(), "assigned");
        assert_eq!(JobState::Scheduled.category(), "scheduled");
        assert_eq!(JobState::Running.category(), "running");
        assert_eq!(JobState::Succeeded.category(), "completed");
        assert_eq!(JobState::Failed.category(), "failed");
        assert_eq!(JobState::Cancelled.category(), "cancelled");
        assert_eq!(JobState::Timeout.category(), "timeout");
    }

    // WorkerState tests
    #[test]
    fn test_worker_state_from_str() {
        assert_eq!(
            "CREATING".parse::<WorkerState>().unwrap(),
            WorkerState::Creating
        );
        assert_eq!("READY".parse::<WorkerState>().unwrap(), WorkerState::Ready);
        assert_eq!("BUSY".parse::<WorkerState>().unwrap(), WorkerState::Busy);
        assert_eq!(
            "TERMINATED".parse::<WorkerState>().unwrap(),
            WorkerState::Terminated
        );

        assert!("INVALID".parse::<WorkerState>().is_err());
    }

    #[test]
    fn test_worker_state_try_from_i32() {
        assert_eq!(WorkerState::try_from(0).unwrap(), WorkerState::Creating);
        assert_eq!(WorkerState::try_from(1).unwrap(), WorkerState::Ready);
        assert_eq!(WorkerState::try_from(2).unwrap(), WorkerState::Busy);
        assert_eq!(WorkerState::try_from(3).unwrap(), WorkerState::Terminated);

        assert!(WorkerState::try_from(99).is_err());
    }

    #[test]
    fn test_worker_state_into_i32() {
        assert_eq!(i32::from(&WorkerState::Creating), 0);
        assert_eq!(i32::from(&WorkerState::Ready), 1);
        assert_eq!(i32::from(&WorkerState::Busy), 2);
        assert_eq!(i32::from(&WorkerState::Terminated), 3);
    }

    // ProviderStatus tests
    #[test]
    fn test_provider_status_from_str() {
        assert_eq!(
            "ACTIVE".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Active
        );
        assert_eq!(
            "MAINTENANCE".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Maintenance
        );
        assert_eq!(
            "DISABLED".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Disabled
        );
        assert_eq!(
            "OVERLOADED".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Overloaded
        );
        assert_eq!(
            "UNHEALTHY".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Unhealthy
        );
        assert_eq!(
            "DEGRADED".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Degraded
        );

        assert!("INVALID".parse::<ProviderStatus>().is_err());
    }

    #[test]
    fn test_provider_status_try_from_i32() {
        assert_eq!(ProviderStatus::try_from(0).unwrap(), ProviderStatus::Active);
        assert_eq!(
            ProviderStatus::try_from(1).unwrap(),
            ProviderStatus::Maintenance
        );
        assert_eq!(
            ProviderStatus::try_from(2).unwrap(),
            ProviderStatus::Disabled
        );
        assert_eq!(
            ProviderStatus::try_from(3).unwrap(),
            ProviderStatus::Overloaded
        );
        assert_eq!(
            ProviderStatus::try_from(4).unwrap(),
            ProviderStatus::Unhealthy
        );
        assert_eq!(
            ProviderStatus::try_from(5).unwrap(),
            ProviderStatus::Degraded
        );

        assert!(ProviderStatus::try_from(99).is_err());
    }

    #[test]
    fn test_provider_status_into_i32() {
        assert_eq!(i32::from(&ProviderStatus::Active), 0);
        assert_eq!(i32::from(&ProviderStatus::Maintenance), 1);
        assert_eq!(i32::from(&ProviderStatus::Disabled), 2);
        assert_eq!(i32::from(&ProviderStatus::Overloaded), 3);
        assert_eq!(i32::from(&ProviderStatus::Unhealthy), 4);
        assert_eq!(i32::from(&ProviderStatus::Degraded), 5);
    }

    // ExecutionStatus tests
    #[test]
    fn test_execution_status_from_str() {
        assert_eq!(
            "SUBMITTED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Submitted
        );
        assert_eq!(
            "QUEUED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Queued
        );
        assert_eq!(
            "RUNNING".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Running
        );
        assert_eq!(
            "SUCCEEDED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Succeeded
        );
        assert_eq!(
            "FAILED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Failed
        );
        assert_eq!(
            "CANCELLED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Cancelled
        );
        assert_eq!(
            "TIMED_OUT".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::TimedOut
        );

        assert!("INVALID".parse::<ExecutionStatus>().is_err());
    }

    #[test]
    fn test_execution_status_try_from_i32() {
        assert_eq!(
            ExecutionStatus::try_from(0).unwrap(),
            ExecutionStatus::Submitted
        );
        assert_eq!(
            ExecutionStatus::try_from(1).unwrap(),
            ExecutionStatus::Queued
        );
        assert_eq!(
            ExecutionStatus::try_from(2).unwrap(),
            ExecutionStatus::Running
        );
        assert_eq!(
            ExecutionStatus::try_from(3).unwrap(),
            ExecutionStatus::Succeeded
        );
        assert_eq!(
            ExecutionStatus::try_from(4).unwrap(),
            ExecutionStatus::Failed
        );
        assert_eq!(
            ExecutionStatus::try_from(5).unwrap(),
            ExecutionStatus::Cancelled
        );
        assert_eq!(
            ExecutionStatus::try_from(6).unwrap(),
            ExecutionStatus::TimedOut
        );

        assert!(ExecutionStatus::try_from(99).is_err());
    }

    #[test]
    fn test_execution_status_into_i32() {
        assert_eq!(i32::from(&ExecutionStatus::Submitted), 0);
        assert_eq!(i32::from(&ExecutionStatus::Queued), 1);
        assert_eq!(i32::from(&ExecutionStatus::Running), 2);
        assert_eq!(i32::from(&ExecutionStatus::Succeeded), 3);
        assert_eq!(i32::from(&ExecutionStatus::Failed), 4);
        assert_eq!(i32::from(&ExecutionStatus::Cancelled), 5);
        assert_eq!(i32::from(&ExecutionStatus::TimedOut), 6);
    }

    // =========================================================================
    // JobFailureReason tests
    // =========================================================================

    #[test]
    fn test_job_failure_reason_command_not_found() {
        let reason = JobFailureReason::CommandNotFound {
            command: "python3".to_string(),
        };
        assert_eq!(reason.to_string(), "COMMAND_NOT_FOUND: python3");
        // CommandNotFound is classified as user error (command not installed)
        assert!(reason.is_user_error());
        assert!(!reason.is_infrastructure_error());
    }

    #[test]
    fn test_job_failure_reason_permission_denied() {
        let reason = JobFailureReason::PermissionDenied {
            path: "/app/script.sh".to_string(),
            operation: "execute".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "PERMISSION_DENIED: execute on /app/script.sh"
        );
        let actions = reason.suggested_actions();
        assert!(!actions.is_empty());
        assert!(actions[0].contains("/app/script.sh"));
        assert!(reason.is_user_error());
    }

    #[test]
    fn test_job_failure_reason_file_not_found() {
        let reason = JobFailureReason::FileNotFound {
            path: "/data/input.csv".to_string(),
        };
        assert_eq!(reason.to_string(), "FILE_NOT_FOUND: /data/input.csv");
        let actions = reason.suggested_actions();
        assert!(!actions.is_empty());
        assert!(reason.is_user_error());
    }

    #[test]
    fn test_job_failure_reason_execution_timeout() {
        let reason = JobFailureReason::ExecutionTimeout { limit_secs: 3600 };
        assert_eq!(
            reason.to_string(),
            "EXECUTION_TIMEOUT: exceeded 3600 seconds"
        );
        let actions = reason.suggested_actions();
        assert!(!actions.is_empty());
        // ExecutionTimeout is a user configuration issue
        assert!(reason.is_user_error());
    }

    #[test]
    fn test_job_failure_reason_signal_received() {
        let reason = JobFailureReason::SignalReceived { signal: 15 };
        assert_eq!(reason.to_string(), "SIGNAL_RECEIVED: signal 15");
        let actions = reason.suggested_actions();
        assert!(!actions.is_empty());
        assert!(actions[0].contains("SIGTERM"));
    }

    #[test]
    fn test_job_failure_reason_signal_segfault() {
        let reason = JobFailureReason::SignalReceived { signal: 11 };
        let actions = reason.suggested_actions();
        assert!(actions[0].contains("Segmentation fault"));
    }

    #[test]
    fn test_job_failure_reason_non_zero_exit_code() {
        let reason = JobFailureReason::NonZeroExitCode { exit_code: 1 };
        assert_eq!(reason.to_string(), "NON_ZERO_EXIT_CODE: exit code 1");
    }

    #[test]
    fn test_job_failure_reason_infrastructure_error() {
        let reason = JobFailureReason::InfrastructureError {
            message: "Connection lost".to_string(),
        };
        assert_eq!(reason.to_string(), "INFRASTRUCTURE_ERROR: Connection lost");
        assert!(reason.is_infrastructure_error());
    }

    #[test]
    fn test_job_failure_reason_validation_error() {
        let reason = JobFailureReason::ValidationError {
            field: "command".to_string(),
            reason: "must not be empty".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "VALIDATION_ERROR: command - must not be empty"
        );
        assert!(reason.is_user_error());
    }

    #[test]
    fn test_job_failure_reason_configuration_error() {
        let reason = JobFailureReason::ConfigurationError {
            key: "timeout_ms".to_string(),
            message: "invalid value".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "CONFIGURATION_ERROR: timeout_ms - invalid value"
        );
        assert!(reason.is_user_error());
    }

    #[test]
    fn test_job_failure_reason_secret_injection_error() {
        let reason = JobFailureReason::SecretInjectionError {
            secret_name: "API_KEY".to_string(),
            message: "permission denied".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "SECRET_INJECTION_ERROR: API_KEY - permission denied"
        );
        assert!(reason.is_user_error());
    }

    #[test]
    fn test_job_failure_reason_io_error() {
        let reason = JobFailureReason::IoError {
            operation: "read".to_string(),
            path: Some("/data/file.txt".to_string()),
            message: "Input/output error".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "IO_ERROR: read on /data/file.txt - Input/output error"
        );
        assert!(reason.is_infrastructure_error());
    }

    #[test]
    fn test_job_failure_reason_io_error_no_path() {
        let reason = JobFailureReason::IoError {
            operation: "write".to_string(),
            path: None,
            message: "Disk full".to_string(),
        };
        assert_eq!(reason.to_string(), "IO_ERROR: write - Disk full");
    }

    #[test]
    fn test_job_failure_reason_unknown() {
        let reason = JobFailureReason::Unknown {
            message: "Something unexpected happened".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "UNKNOWN_ERROR: Something unexpected happened"
        );
    }

    #[test]
    fn test_job_failure_reason_suggested_actions_not_empty() {
        let reasons = vec![
            JobFailureReason::CommandNotFound {
                command: "test".to_string(),
            },
            JobFailureReason::PermissionDenied {
                path: "/test".to_string(),
                operation: "read".to_string(),
            },
            JobFailureReason::FileNotFound {
                path: "/test".to_string(),
            },
            JobFailureReason::ProcessSpawnFailed {
                message: "test".to_string(),
            },
            JobFailureReason::ExecutionTimeout { limit_secs: 100 },
            JobFailureReason::SignalReceived { signal: 9 },
            JobFailureReason::NonZeroExitCode { exit_code: 1 },
            JobFailureReason::InfrastructureError {
                message: "test".to_string(),
            },
            JobFailureReason::ValidationError {
                field: "test".to_string(),
                reason: "test".to_string(),
            },
            JobFailureReason::ConfigurationError {
                key: "test".to_string(),
                message: "test".to_string(),
            },
            JobFailureReason::SecretInjectionError {
                secret_name: "test".to_string(),
                message: "test".to_string(),
            },
            JobFailureReason::IoError {
                operation: "test".to_string(),
                path: None,
                message: "test".to_string(),
            },
            JobFailureReason::Unknown {
                message: "test".to_string(),
            },
        ];

        for reason in reasons {
            let actions = reason.suggested_actions();
            assert!(
                !actions.is_empty(),
                "Expected non-empty actions for {:?}",
                reason
            );
        }
    }

    #[test]
    fn test_job_failure_reason_serialization() {
        let reason = JobFailureReason::PermissionDenied {
            path: "/app/script.sh".to_string(),
            operation: "execute".to_string(),
        };

        let serialized = serde_json::to_string(&reason).expect("Failed to serialize");
        let deserialized: JobFailureReason =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(reason, deserialized);
    }

    // =========================================================================
    // DispatchFailureReason tests
    // =========================================================================

    #[test]
    fn test_dispatch_failure_reason_worker_not_ready() {
        let reason = DispatchFailureReason::WorkerNotReady {
            current_state: "Busy".to_string(),
        };
        assert_eq!(reason.to_string(), "WORKER_NOT_READY: worker in state Busy");
    }

    #[test]
    fn test_dispatch_failure_reason_communication_timeout() {
        let reason = DispatchFailureReason::CommunicationTimeout { timeout_ms: 5000 };
        assert_eq!(reason.to_string(), "COMMUNICATION_TIMEOUT: exceeded 5000ms");
    }

    #[test]
    fn test_dispatch_failure_reason_worker_crashed() {
        let reason = DispatchFailureReason::WorkerCrashed;
        assert_eq!(reason.to_string(), "WORKER_CRASHED");
    }

    #[test]
    fn test_dispatch_failure_reason_channel_closed() {
        let reason = DispatchFailureReason::ChannelClosed;
        assert_eq!(reason.to_string(), "CHANNEL_CLOSED");
    }

    #[test]
    fn test_dispatch_failure_reason_protocol_error() {
        let reason = DispatchFailureReason::ProtocolError {
            message: "Malformed message".to_string(),
        };
        assert_eq!(reason.to_string(), "PROTOCOL_ERROR: Malformed message");
    }

    #[test]
    fn test_dispatch_failure_reason_network_error() {
        let reason = DispatchFailureReason::NetworkError {
            message: "Connection reset".to_string(),
        };
        assert_eq!(reason.to_string(), "NETWORK_ERROR: Connection reset");
    }

    #[test]
    fn test_dispatch_failure_reason_serialization() {
        let reason = DispatchFailureReason::WorkerNotReady {
            current_state: "Terminating".to_string(),
        };

        let serialized = serde_json::to_string(&reason).expect("Failed to serialize");
        let deserialized: DispatchFailureReason =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(reason, deserialized);
    }

    // =========================================================================
    // ProvisioningFailureReason tests
    // =========================================================================

    #[test]
    fn test_provisioning_failure_reason_image_pull_failed() {
        let reason = ProvisioningFailureReason::ImagePullFailed {
            image: "hodei-worker:latest".to_string(),
            message: "not found".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "IMAGE_PULL_FAILED: hodei-worker:latest - not found"
        );
    }

    #[test]
    fn test_provisioning_failure_reason_resource_allocation_failed() {
        let reason = ProvisioningFailureReason::ResourceAllocationFailed {
            resource: "memory".to_string(),
            message: "insufficient".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "RESOURCE_ALLOCATION_FAILED: memory - insufficient"
        );
    }

    #[test]
    fn test_provisioning_failure_reason_network_setup_failed() {
        let reason = ProvisioningFailureReason::NetworkSetupFailed {
            message: "timeout".to_string(),
        };
        assert_eq!(reason.to_string(), "NETWORK_SETUP_FAILED: timeout");
    }

    #[test]
    fn test_provisioning_failure_reason_authentication_failed() {
        let reason = ProvisioningFailureReason::AuthenticationFailed {
            message: "invalid credentials".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "AUTHENTICATION_FAILED: invalid credentials"
        );
    }

    #[test]
    fn test_provisioning_failure_reason_timeout() {
        let reason = ProvisioningFailureReason::Timeout;
        assert_eq!(reason.to_string(), "PROVISIONING_TIMEOUT");
    }

    #[test]
    fn test_provisioning_failure_reason_provider_unavailable() {
        let reason = ProvisioningFailureReason::ProviderUnavailable;
        assert_eq!(reason.to_string(), "PROVIDER_UNAVAILABLE");
    }

    #[test]
    fn test_provisioning_failure_reason_internal_error() {
        let reason = ProvisioningFailureReason::InternalError {
            message: "panic in provider".to_string(),
        };
        assert_eq!(reason.to_string(), "INTERNAL_ERROR: panic in provider");
    }

    #[test]
    fn test_provisioning_failure_reason_invalid_configuration() {
        let reason = ProvisioningFailureReason::InvalidConfiguration {
            field: "cpu_cores".to_string(),
            reason: "must be positive".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "INVALID_CONFIGURATION: cpu_cores - must be positive"
        );
    }

    #[test]
    fn test_provisioning_failure_reason_serialization() {
        let reason = ProvisioningFailureReason::ImagePullFailed {
            image: "test:latest".to_string(),
            message: "error".to_string(),
        };

        let serialized = serde_json::to_string(&reason).expect("Failed to serialize");
        let deserialized: ProvisioningFailureReason =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(reason, deserialized);
    }

    // =========================================================================
    // SchedulingFailureReason tests
    // =========================================================================

    #[test]
    fn test_scheduling_failure_reason_no_providers_available() {
        let reason = SchedulingFailureReason::NoProvidersAvailable;
        assert_eq!(reason.to_string(), "NO_PROVIDERS_AVAILABLE");
    }

    #[test]
    fn test_scheduling_failure_reason_no_matching_providers() {
        let reason = SchedulingFailureReason::NoMatchingProviders {
            requirement: "gpu: true".to_string(),
        };
        assert_eq!(reason.to_string(), "NO_MATCHING_PROVIDERS: gpu: true");
    }

    #[test]
    fn test_scheduling_failure_reason_provider_overloaded() {
        let reason = SchedulingFailureReason::ProviderOverloaded;
        assert_eq!(reason.to_string(), "PROVIDER_OVERLOADED");
    }

    #[test]
    fn test_scheduling_failure_reason_provider_unhealthy() {
        let reason = SchedulingFailureReason::ProviderUnhealthy {
            provider_id: "provider-123".to_string(),
        };
        assert_eq!(reason.to_string(), "PROVIDER_UNHEALTHY: provider-123");
    }

    #[test]
    fn test_scheduling_failure_reason_resource_constraints_not_met() {
        let reason = SchedulingFailureReason::ResourceConstraintsNotMet {
            resource: "memory".to_string(),
            required: "16GB".to_string(),
            available: "8GB".to_string(),
        };
        assert_eq!(
            reason.to_string(),
            "RESOURCE_CONSTRAINTS_NOT_MET: memory required=16GB, available=8GB"
        );
    }

    #[test]
    fn test_scheduling_failure_reason_internal_error() {
        let reason = SchedulingFailureReason::InternalError {
            message: "scheduler panic".to_string(),
        };
        assert_eq!(reason.to_string(), "INTERNAL_ERROR: scheduler panic");
    }

    #[test]
    fn test_scheduling_failure_reason_job_cancelled() {
        let reason = SchedulingFailureReason::JobCancelled;
        assert_eq!(reason.to_string(), "JOB_CANCELLED");
    }

    #[test]
    fn test_scheduling_failure_reason_job_expired() {
        let reason = SchedulingFailureReason::JobExpired;
        assert_eq!(reason.to_string(), "JOB_EXPIRED");
    }

    #[test]
    fn test_scheduling_failure_reason_serialization() {
        let reason = SchedulingFailureReason::NoMatchingProviders {
            requirement: "label: gpu".to_string(),
        };

        let serialized = serde_json::to_string(&reason).expect("Failed to serialize");
        let deserialized: SchedulingFailureReason =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(reason, deserialized);
    }

    // =========================================================================
    // ProviderErrorType tests
    // =========================================================================

    #[test]
    fn test_provider_error_type_display() {
        assert_eq!(
            ProviderErrorType::ConnectionLost.to_string(),
            "CONNECTION_LOST"
        );
        assert_eq!(
            ProviderErrorType::AuthenticationFailed.to_string(),
            "AUTHENTICATION_FAILED"
        );
        assert_eq!(
            ProviderErrorType::ResourceLimitExceeded.to_string(),
            "RESOURCE_LIMIT_EXCEEDED"
        );
        assert_eq!(
            ProviderErrorType::WorkerNotFound.to_string(),
            "WORKER_NOT_FOUND"
        );
        assert_eq!(
            ProviderErrorType::ProvisioningFailed.to_string(),
            "PROVISIONING_FAILED"
        );
        assert_eq!(ProviderErrorType::Timeout.to_string(), "TIMEOUT");
        assert_eq!(
            ProviderErrorType::ConfigurationError.to_string(),
            "CONFIGURATION_ERROR"
        );
        assert_eq!(ProviderErrorType::Internal.to_string(), "INTERNAL_ERROR");
    }

    #[test]
    fn test_provider_error_type_serialization() {
        let error_type = ProviderErrorType::ResourceLimitExceeded;

        let serialized = serde_json::to_string(&error_type).expect("Failed to serialize");
        let deserialized: ProviderErrorType =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(error_type, deserialized);
    }

    // =========================================================================
    // WorkerState transition tests (US-30.1)
    // =========================================================================

    #[test]
    fn test_worker_state_valid_transitions_creating() {
        // Crash-Only: Creating -> Ready (éxito) o Terminated (error)
        assert!(WorkerState::Creating.can_transition_to(&WorkerState::Ready));
        assert!(WorkerState::Creating.can_transition_to(&WorkerState::Terminated));
        assert!(!WorkerState::Creating.can_transition_to(&WorkerState::Busy));
        assert!(!WorkerState::Creating.can_transition_to(&WorkerState::Creating));
    }

    #[test]
    fn test_worker_state_valid_transitions_ready() {
        // Ready -> Busy (job asignado) o Terminated (terminación directa)
        assert!(WorkerState::Ready.can_transition_to(&WorkerState::Busy));
        assert!(WorkerState::Ready.can_transition_to(&WorkerState::Terminated));
        assert!(!WorkerState::Ready.can_transition_to(&WorkerState::Creating));
        assert!(!WorkerState::Ready.can_transition_to(&WorkerState::Ready));
    }

    #[test]
    fn test_worker_state_valid_transitions_busy() {
        // Busy -> Terminated (job completado o error, worker efímero)
        assert!(WorkerState::Busy.can_transition_to(&WorkerState::Terminated));
        assert!(!WorkerState::Busy.can_transition_to(&WorkerState::Ready)); // Workers son efímeros
        assert!(!WorkerState::Busy.can_transition_to(&WorkerState::Creating));
        assert!(!WorkerState::Busy.can_transition_to(&WorkerState::Busy));
    }

    #[test]
    fn test_worker_state_invalid_same_state() {
        assert!(!WorkerState::Ready.can_transition_to(&WorkerState::Ready));
        assert!(!WorkerState::Busy.can_transition_to(&WorkerState::Busy));
        assert!(!WorkerState::Creating.can_transition_to(&WorkerState::Creating));
        assert!(!WorkerState::Terminated.can_transition_to(&WorkerState::Terminated));
    }

    #[test]
    fn test_worker_state_invalid_backwards_transitions() {
        assert!(!WorkerState::Ready.can_transition_to(&WorkerState::Creating));
        assert!(!WorkerState::Busy.can_transition_to(&WorkerState::Creating));
        assert!(!WorkerState::Busy.can_transition_to(&WorkerState::Ready));
    }

    #[test]
    fn test_worker_state_terminal_no_transitions() {
        assert!(!WorkerState::Terminated.can_transition_to(&WorkerState::Ready));
        assert!(!WorkerState::Terminated.can_transition_to(&WorkerState::Creating));
        assert!(!WorkerState::Terminated.can_transition_to(&WorkerState::Busy));
    }

    #[test]
    fn test_worker_state_terminal() {
        assert!(WorkerState::Terminated.is_terminal());
        assert!(!WorkerState::Ready.is_terminal());
        assert!(!WorkerState::Busy.is_terminal());
        assert!(!WorkerState::Creating.is_terminal());
    }

    #[test]
    fn test_worker_state_in_progress() {
        assert!(WorkerState::Creating.is_in_progress());
        assert!(WorkerState::Ready.is_in_progress());
        assert!(WorkerState::Busy.is_in_progress());
        assert!(!WorkerState::Terminated.is_in_progress());
    }

    #[test]
    fn test_worker_state_is_terminating() {
        assert!(WorkerState::Terminated.is_terminating());
        assert!(!WorkerState::Ready.is_terminating());
        assert!(!WorkerState::Busy.is_terminating());
        assert!(!WorkerState::Creating.is_terminating());
    }
}
