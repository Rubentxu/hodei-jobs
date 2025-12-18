// Job Execution Bounded Context
// Maneja el lifecycle de jobs, especificaciones y colas
// Alineado con PRD v6.0

use crate::shared_kernel::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

/// Tipo de comando a ejecutar (PRD v6.0)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommandType {
    /// Comando shell con argumentos
    Shell { cmd: String, args: Vec<String> },
    /// Script inline con interprete
    Script {
        interpreter: String,
        content: String,
    },
}

impl CommandType {
    /// Crea un comando shell simple
    pub fn shell(cmd: impl Into<String>) -> Self {
        CommandType::Shell {
            cmd: cmd.into(),
            args: vec![],
        }
    }

    /// Crea un comando shell con argumentos
    pub fn shell_with_args(cmd: impl Into<String>, args: Vec<String>) -> Self {
        CommandType::Shell {
            cmd: cmd.into(),
            args,
        }
    }

    /// Crea un script inline
    pub fn script(interpreter: impl Into<String>, content: impl Into<String>) -> Self {
        CommandType::Script {
            interpreter: interpreter.into(),
            content: content.into(),
        }
    }

    /// Convierte a vector de strings para ejecución
    pub fn to_command_vec(&self) -> Vec<String> {
        match self {
            CommandType::Shell { cmd, args } => {
                let mut v = vec![cmd.clone()];
                v.extend(args.clone());
                v
            }
            CommandType::Script {
                interpreter,
                content,
            } => {
                vec![interpreter.clone(), "-c".to_string(), content.clone()]
            }
        }
    }
}

/// Operador para constraints de scheduling (PRD v6.0)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstraintOperator {
    /// Igual a
    Eq,
    /// No igual a
    Ne,
    /// Mayor que
    Gt,
    /// Menor que
    Lt,
    /// Contenido en lista
    In,
    /// No contenido en lista
    NotIn,
}

/// Restricción para scheduling de jobs (PRD v6.0)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    /// Clave de la restricción (ej: "os", "memory", "provider")
    pub key: String,
    /// Operador de comparación
    pub operator: ConstraintOperator,
    /// Valor a comparar
    pub value: String,
}

impl Constraint {
    pub fn new(
        key: impl Into<String>,
        operator: ConstraintOperator,
        value: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            operator,
            value: value.into(),
        }
    }

    /// Constraint de igualdad
    pub fn eq(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new(key, ConstraintOperator::Eq, value)
    }

    /// Constraint de provider específico
    pub fn provider(provider_name: impl Into<String>) -> Self {
        Self::eq("provider", provider_name)
    }
}

/// Fuente de artefacto (S3 URL) para inputs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactSource {
    /// URL del artefacto (ej: s3://bucket/path)
    pub url: String,
    /// Ruta destino en el worker
    pub dest_path: String,
}

/// Destino de artefacto (S3 URL) para outputs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactDest {
    /// Ruta origen en el worker
    pub src_path: String,
    /// URL destino (ej: s3://bucket/path)
    pub url: String,
}

/// Especificación completa de un job (alineado con PRD v6.0)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSpec {
    /// Comando a ejecutar (PRD v6.0: CommandType)
    pub command: CommandType,
    /// Variables de entorno
    pub env: HashMap<String, String>,
    /// Artefactos de entrada (S3 URLs)
    pub inputs: Vec<ArtifactSource>,
    /// Artefactos de salida (S3 URLs)
    pub outputs: Vec<ArtifactDest>,
    /// Restricciones de scheduling
    pub constraints: Vec<Constraint>,
    /// Recursos requeridos
    pub resources: JobResources,
    /// Timeout del job en milisegundos
    pub timeout_ms: u64,
    /// Imagen Docker (opcional, para override)
    pub image: Option<String>,
    /// Directorio de trabajo (opcional)
    pub working_dir: Option<String>,
    /// Preferencias del usuario
    pub preferences: JobPreferences,
}

impl JobSpec {
    /// Crea un JobSpec con comando shell simple (retrocompatibilidad)
    pub fn new(command: Vec<String>) -> Self {
        let cmd_type = if command.is_empty() {
            CommandType::shell("echo")
        } else {
            CommandType::shell_with_args(command[0].clone(), command[1..].to_vec())
        };
        Self {
            command: cmd_type,
            env: HashMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            constraints: Vec::new(),
            resources: JobResources::default(),
            timeout_ms: 300_000, // 5 minutos por defecto
            image: None,
            working_dir: None,
            preferences: JobPreferences::default(),
        }
    }

    /// Crea un JobSpec con CommandType
    pub fn with_command(command: CommandType) -> Self {
        Self {
            command,
            env: HashMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            constraints: Vec::new(),
            resources: JobResources::default(),
            timeout_ms: 300_000,
            image: None,
            working_dir: None,
            preferences: JobPreferences::default(),
        }
    }

    /// Añade una variable de entorno
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Añade un artefacto de entrada
    pub fn with_input(mut self, url: impl Into<String>, dest_path: impl Into<String>) -> Self {
        self.inputs.push(ArtifactSource {
            url: url.into(),
            dest_path: dest_path.into(),
        });
        self
    }

    /// Añade un artefacto de salida
    pub fn with_output(mut self, src_path: impl Into<String>, url: impl Into<String>) -> Self {
        self.outputs.push(ArtifactDest {
            src_path: src_path.into(),
            url: url.into(),
        });
        self
    }

    /// Añade una constraint
    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Obtiene el comando como vector para ejecución
    pub fn command_vec(&self) -> Vec<String> {
        self.command.to_command_vec()
    }
}

/// Recursos requeridos para un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobResources {
    /// CPU en vCPUs
    pub cpu_cores: f32,
    /// Memoria en MB
    pub memory_mb: u64,
    /// Almacenamiento en MB
    pub storage_mb: u64,
    /// GPU requerida
    pub gpu_required: bool,
    /// Arquitectura requerida
    pub architecture: String,
}

impl Default for JobResources {
    fn default() -> Self {
        Self {
            cpu_cores: 1.0,
            memory_mb: 512,
            storage_mb: 1024,
            gpu_required: false,
            architecture: "x86_64".to_string(),
        }
    }
}

/// Archivo de entrada para un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputFile {
    /// Nombre del archivo
    pub name: String,
    /// Contenido del archivo
    pub content: String,
    /// Permisos del archivo
    pub permissions: String,
}

/// Preferencias del usuario para un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobPreferences {
    /// Provider preferido (opcional)
    pub preferred_provider: Option<String>,
    /// Región preferida (opcional)
    pub preferred_region: Option<String>,
    /// Presupuesto máximo (opcional)
    pub max_budget: Option<f64>,
    /// Prioridad del job
    pub priority: JobPriority,
    /// Permite reintentos
    pub allow_retry: bool,
}

impl Default for JobPreferences {
    fn default() -> Self {
        Self {
            preferred_provider: None,
            preferred_region: None,
            max_budget: None,
            priority: JobPriority::Normal,
            allow_retry: true,
        }
    }
}

/// Prioridad de un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl fmt::Display for JobPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobPriority::Low => write!(f, "LOW"),
            JobPriority::Normal => write!(f, "NORMAL"),
            JobPriority::High => write!(f, "HIGH"),
            JobPriority::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Agregado Job - maneja el lifecycle completo
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// Identificador único del job
    pub id: JobId,
    /// Especificación del job
    pub spec: JobSpec,
    /// Estado actual del job
    pub state: JobState,
    /// Provider seleccionado (si aplica)
    pub selected_provider: Option<ProviderId>,
    /// Contexto de ejecución (si el job está en ejecución)
    pub execution_context: Option<ExecutionContext>,
    /// Número de intentos actuales
    pub attempts: u32,
    /// Máximo número de intentos
    pub max_attempts: u32,
    /// Fecha de creación
    pub created_at: DateTime<Utc>,
    /// Fecha de inicio de ejecución
    pub started_at: Option<DateTime<Utc>>,
    /// Fecha de finalización
    pub completed_at: Option<DateTime<Utc>>,
    /// Resultado del job (si completado)
    pub result: Option<JobResult>,
    /// Mensaje de error (si falló)
    pub error_message: Option<String>,
    /// Metadatos adicionales
    pub metadata: HashMap<String, String>,
}

impl Job {
    /// Crea un nuevo job
    pub fn new(id: JobId, spec: JobSpec) -> Self {
        Self {
            id,
            spec,
            state: JobState::Pending,
            selected_provider: None,
            execution_context: None,
            attempts: 0,
            max_attempts: 3,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
            error_message: None,
            metadata: HashMap::new(),
        }
    }

    /// Pone el job en cola
    pub fn queue(&mut self) -> Result<()> {
        match self.state {
            JobState::Pending => Ok(()),
            _ => Err(DomainError::InvalidStateTransition {
                from: self.state.clone(),
                to: JobState::Pending,
            }),
        }
    }

    /// Envía el job a un provider (PRD v6.0: Scheduled)
    pub fn submit_to_provider(
        &mut self,
        provider_id: ProviderId,
        context: ExecutionContext,
    ) -> Result<()> {
        match self.state {
            JobState::Pending | JobState::Scheduled => {
                self.state = JobState::Scheduled;
                self.selected_provider = Some(provider_id);
                self.execution_context = Some(context);
                self.started_at = Some(Utc::now());
                Ok(())
            }
            _ => Err(DomainError::InvalidStateTransition {
                from: self.state.clone(),
                to: JobState::Scheduled,
            }),
        }
    }

    /// Marca el job como ejecutándose
    pub fn mark_running(&mut self) -> Result<()> {
        match self.state {
            JobState::Scheduled => {
                self.state = JobState::Running;
                Ok(())
            }
            _ => Err(DomainError::InvalidStateTransition {
                from: self.state.clone(),
                to: JobState::Running,
            }),
        }
    }

    /// Completa el job exitosamente
    pub fn complete(&mut self, result: JobResult) -> Result<()> {
        match self.state {
            JobState::Running | JobState::Scheduled => {
                self.state = match &result {
                    JobResult::Success { .. } => JobState::Succeeded,
                    JobResult::Failed { .. } => JobState::Failed,
                    JobResult::Cancelled => JobState::Cancelled,
                    JobResult::Timeout => JobState::Timeout,
                };
                self.completed_at = Some(Utc::now());
                self.result = Some(result);
                Ok(())
            }
            _ => Err(DomainError::InvalidStateTransition {
                from: self.state.clone(),
                to: JobState::Succeeded,
            }),
        }
    }

    /// Marca el job como fallido
    pub fn fail(&mut self, error_message: String) -> Result<()> {
        self.state = JobState::Failed;
        self.completed_at = Some(Utc::now());
        self.error_message = Some(error_message);
        self.attempts += 1;
        Ok(())
    }

    /// Cancela el job
    pub fn cancel(&mut self) -> Result<()> {
        match self.state {
            JobState::Pending | JobState::Scheduled | JobState::Running => {
                self.state = JobState::Cancelled;
                self.completed_at = Some(Utc::now());
                Ok(())
            }
            _ => Err(DomainError::InvalidStateTransition {
                from: self.state.clone(),
                to: JobState::Cancelled,
            }),
        }
    }

    /// Verifica si el job puede ser reintentado
    pub fn can_retry(&self) -> bool {
        self.attempts < self.max_attempts
            && matches!(self.state, JobState::Failed | JobState::Timeout)
            && self.spec.preferences.allow_retry
    }

    /// Prepara el job para un nuevo intento
    pub fn prepare_retry(&mut self) -> Result<()> {
        if !self.can_retry() {
            return Err(DomainError::MaxAttemptsExceeded {
                job_id: self.id.clone(),
                max_attempts: self.max_attempts,
            });
        }

        self.state = JobState::Pending;
        self.selected_provider = None;
        self.execution_context = None;
        self.started_at = None;
        self.completed_at = None;
        self.result = None;
        self.error_message = None;
        self.attempts += 1;

        Ok(())
    }

    /// Verifica si el job ha expirado
    pub fn has_expired(&self) -> bool {
        if let Some(started_at) = self.started_at {
            let elapsed = Utc::now().signed_duration_since(started_at);
            elapsed.num_milliseconds() as u64 > self.spec.timeout_ms
        } else {
            false
        }
    }

    /// Obtiene la duración de ejecución
    pub fn execution_duration(&self) -> Option<Duration> {
        if let (Some(started), Some(completed)) = (self.started_at, self.completed_at) {
            Some(
                completed
                    .signed_duration_since(started)
                    .to_std()
                    .unwrap_or_default(),
            )
        } else {
            None
        }
    }
}

impl Aggregate for Job {
    type Id = JobId;

    fn aggregate_id(&self) -> &Self::Id {
        &self.id
    }
}

/// Contexto de ejecución del job en un provider específico
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// ID del job
    pub job_id: JobId,
    /// ID del provider
    pub provider_id: ProviderId,
    /// ID de ejecución específico del provider
    pub provider_execution_id: String,
    /// Fecha de envío
    pub submitted_at: DateTime<Utc>,
    /// Fecha de inicio (opcional)
    pub started_at: Option<DateTime<Utc>>,
    /// Fecha de finalización (opcional)
    pub completed_at: Option<DateTime<Utc>>,
    /// Estado actual de la ejecución
    pub status: ExecutionStatus,
    /// Resultado de la ejecución (si disponible)
    pub result: Option<JobResult>,
    /// Metadatos adicionales del provider
    pub metadata: HashMap<String, String>,
}

impl ExecutionContext {
    pub fn new(job_id: JobId, provider_id: ProviderId, provider_execution_id: String) -> Self {
        Self {
            job_id,
            provider_id,
            provider_execution_id,
            submitted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            status: ExecutionStatus::Submitted,
            result: None,
            metadata: HashMap::new(),
        }
    }

    /// Actualiza el estado de la ejecución
    pub fn update_status(&mut self, status: ExecutionStatus) {
        self.status = status.clone();

        match status {
            ExecutionStatus::Running if self.started_at.is_none() => {
                self.started_at = Some(Utc::now());
            }
            ExecutionStatus::Succeeded | ExecutionStatus::Failed | ExecutionStatus::Cancelled => {
                if self.completed_at.is_none() {
                    self.completed_at = Some(Utc::now());
                }
            }
            _ => {}
        }
    }
}

/// Trait para repositorios de jobs
#[async_trait::async_trait]
pub trait JobRepository: Send + Sync {
    async fn save(&self, job: &Job) -> Result<()>;
    async fn find_by_id(&self, job_id: &JobId) -> Result<Option<Job>>;
    async fn find_by_state(&self, state: &JobState) -> Result<Vec<Job>>;
    async fn find_pending(&self) -> Result<Vec<Job>>;
    async fn find_all(&self, limit: usize, offset: usize) -> Result<(Vec<Job>, usize)>;
    async fn find_by_execution_id(&self, execution_id: &str) -> Result<Option<Job>>;
    async fn delete(&self, job_id: &JobId) -> Result<()>;
    async fn update(&self, job: &Job) -> Result<()>;
}

/// Trait para colas de jobs
#[async_trait::async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: Job) -> Result<()>;
    async fn dequeue(&self) -> Result<Option<Job>>;
    async fn peek(&self) -> Result<Option<Job>>;
    async fn len(&self) -> Result<usize>;
    async fn is_empty(&self) -> Result<bool>;
    async fn clear(&self) -> Result<()>;
}

/// Servicio del dominio para tracking de estado de jobs
pub struct JobStatusTracker {
    // Servicios de repositorio
    job_repository: Box<dyn JobRepository>,
}

impl JobStatusTracker {
    pub fn new(job_repository: Box<dyn JobRepository>) -> Self {
        Self { job_repository }
    }

    /// Actualiza el estado de un job
    pub async fn update_job_status(
        &self,
        job_id: &JobId,
        new_status: ExecutionStatus,
        result: Option<JobResult>,
    ) -> Result<()> {
        let mut job = self
            .job_repository
            .find_by_id(job_id)
            .await?
            .ok_or_else(|| DomainError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // Actualizar contexto de ejecución si existe
        if let Some(ref mut context) = job.execution_context {
            context.update_status(new_status.clone());
            if let Some(ref job_result) = result {
                context.result = Some(job_result.clone());
            }
        }

        // Mapear estado de ejecución a estado de job
        let new_job_state = match new_status {
            ExecutionStatus::Running => JobState::Running,
            ExecutionStatus::Succeeded => JobState::Succeeded,
            ExecutionStatus::Failed => JobState::Failed,
            ExecutionStatus::Cancelled => JobState::Cancelled,
            ExecutionStatus::TimedOut => JobState::Timeout,
            ExecutionStatus::Submitted | ExecutionStatus::Queued => JobState::Scheduled,
        };

        if job.state != new_job_state {
            job.state = new_job_state;
        }

        if let Some(ref job_result) = result {
            job.result = Some(job_result.clone());
            job.completed_at = Some(Utc::now());
        }

        self.job_repository.update(&job).await?;
        Ok(())
    }

    /// Obtiene el estado actual de un job
    pub async fn get_job_status(
        &self,
        job_id: &JobId,
    ) -> Result<Option<(JobState, Option<JobResult>)>> {
        let job = self.job_repository.find_by_id(job_id).await?;

        Ok(job.map(|job| (job.state, job.result)))
    }

    /// Limpia jobs expirados
    pub async fn cleanup_expired_jobs(&self) -> Result<Vec<JobId>> {
        let mut expired_job_ids = Vec::new();

        let pending_jobs = self
            .job_repository
            .find_by_state(&JobState::Pending)
            .await?;
        for job in pending_jobs {
            if job.has_expired() {
                expired_job_ids.push(job.id.clone());
                // Marcar como expirado
                // TODO: Implementar en job
            }
        }

        Ok(expired_job_ids)
    }
}
