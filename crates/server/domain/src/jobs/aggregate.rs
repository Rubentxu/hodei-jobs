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
    /// Entrada estándar (opcional, para secrets)
    pub stdin: Option<String>,
    /// Preferencias del usuario
    pub preferences: JobPreferences,
}

impl JobSpec {
    /// Detecta si una cadena parece ser contenido de script (contiene nuevas líneas o shebang)
    fn looks_like_script(content: &str) -> bool {
        // Es script si:
        // 1. Contiene nuevas líneas (es multilinea)
        // 2. O comienza con shebang (#!)
        content.contains('\n') || content.starts_with("#!")
    }

    /// Builder Pattern: Crea un JobSpec con comando shell simple (retrocompatibilidad)
    pub fn new(command: Vec<String>) -> Self {
        // Detectar si el comando es contenido de script (múltiples líneas o contiene shebang)
        let cmd_type = if command.is_empty() {
            CommandType::shell("echo")
        } else if command.len() == 1 && Self::looks_like_script(&command[0]) {
            // Si hay un solo elemento y parece ser contenido de script, crear CommandType::Script
            let content = command[0].clone();
            // Detectar el interprete del shebang o usar bash por defecto
            let interpreter = if content.starts_with("#!") {
                let shebang_line = content.lines().next().unwrap_or("");
                if let Some(path) = shebang_line.strip_prefix("#!") {
                    let path = path.trim();
                    // Extraer el interprete del path (ej: /bin/bash -> bash)
                    path.split('/').last().unwrap_or("bash").to_string()
                } else {
                    "bash".to_string()
                }
            } else {
                "bash".to_string()
            };
            CommandType::script(interpreter, content)
        } else {
            // Comportamiento original: comando shell con argumentos
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
            stdin: None,
            preferences: JobPreferences::default(),
        }
    }

    /// Builder Pattern: Constructor con CommandType
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
            stdin: None,
            preferences: JobPreferences::default(),
        }
    }

    /// Builder Pattern: Añade variable de entorno
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Builder Pattern: Añade artefacto de entrada
    pub fn with_input(mut self, url: impl Into<String>, dest_path: impl Into<String>) -> Self {
        self.inputs.push(ArtifactSource {
            url: url.into(),
            dest_path: dest_path.into(),
        });
        self
    }

    /// Builder Pattern: Añade artefacto de salida
    pub fn with_output(mut self, src_path: impl Into<String>, url: impl Into<String>) -> Self {
        self.outputs.push(ArtifactDest {
            src_path: src_path.into(),
            url: url.into(),
        });
        self
    }

    /// Builder Pattern: Añade constraint
    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Builder Pattern: Añade stdin
    pub fn with_stdin(mut self, stdin: impl Into<String>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    /// Builder Pattern: Establece recursos
    pub fn with_resources(mut self, cpu_cores: f32, memory_mb: u64, storage_mb: u64) -> Self {
        self.resources.cpu_cores = cpu_cores;
        self.resources.memory_mb = memory_mb;
        self.resources.storage_mb = storage_mb;
        self
    }

    /// Builder Pattern: Establece imagen
    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = Some(image.into());
        self
    }

    /// Builder Pattern: Establece directorio de trabajo
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Builder Pattern: Establece timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Builder Pattern: Establece preferencias
    pub fn with_preferences(mut self, preferences: JobPreferences) -> Self {
        self.preferences = preferences;
        self
    }

    /// Validación de JobSpec (Clean Code - Early Returns)
    pub fn validate(&self) -> Result<()> {
        // Validar comando no vacío
        let cmd_vec = self.command_vec();
        if cmd_vec.is_empty() || cmd_vec.iter().all(|s| s.is_empty()) {
            return Err(DomainError::InvalidJobSpec {
                field: "command".to_string(),
                reason: "Job command cannot be empty".to_string(),
            });
        }

        // Validar timeout > 0
        if self.timeout_ms == 0 {
            return Err(DomainError::InvalidJobSpec {
                field: "timeout_ms".to_string(),
                reason: "Job timeout must be greater than 0".to_string(),
            });
        }

        // Validar recursos
        if self.resources.cpu_cores <= 0.0 {
            return Err(DomainError::InvalidJobSpec {
                field: "cpu_cores".to_string(),
                reason: "CPU cores must be greater than 0".to_string(),
            });
        }

        if self.resources.memory_mb == 0 {
            return Err(DomainError::InvalidJobSpec {
                field: "memory_mb".to_string(),
                reason: "Memory must be greater than 0".to_string(),
            });
        }

        // Validar que no hay paths vacíos en inputs/outputs
        for input in &self.inputs {
            if input.dest_path.is_empty() {
                return Err(DomainError::InvalidJobSpec {
                    field: "inputs.dest_path".to_string(),
                    reason: "Destination path cannot be empty".to_string(),
                });
            }
        }

        for output in &self.outputs {
            if output.src_path.is_empty() {
                return Err(DomainError::InvalidJobSpec {
                    field: "outputs.src_path".to_string(),
                    reason: "Source path cannot be empty".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Obtiene el comando como vector para ejecución
    pub fn command_vec(&self) -> Vec<String> {
        self.command.to_command_vec()
    }

    /// Calcula prioridad del job basada en recursos y preferencias
    pub fn calculate_priority(&self) -> JobPriority {
        // Lógica de negocio para calcular prioridad
        // Factor 1: Preferencia explícita del usuario
        if matches!(self.preferences.priority, JobPriority::Critical) {
            return JobPriority::Critical;
        }

        // Factor 2: Recursos requeridos (jobs que requieren más recursos tienen mayor prioridad)
        let resource_score = self.calculate_resource_score();

        // Factor 3: Tipo de workload (GPU, CPU intensivo, etc.)
        let workload_score = self.calculate_workload_score();

        // Combinar scores
        let total_score = resource_score + workload_score;

        match total_score {
            score if score >= 80.0 => JobPriority::High,
            score if score >= 40.0 => JobPriority::Normal,
            _ => JobPriority::Low,
        }
    }

    /// Determina si el job debe escalarse (auto-scaling)
    pub fn should_escalate(&self, current_queue_depth: usize, threshold: usize) -> bool {
        // Lógica de negocio para escalamiento:
        // 1. Si la cola supera el threshold
        if current_queue_depth >= threshold {
            return true;
        }

        // 2. Jobs críticos siempre activan escalamiento
        if matches!(self.preferences.priority, JobPriority::Critical) {
            return true;
        }

        // 3. Jobs con alta carga de recursos
        let resource_score = self.calculate_resource_score();
        if resource_score >= 80.0 && current_queue_depth >= threshold / 2 {
            return true;
        }

        false
    }

    /// Calcula score de recursos (0-100)
    fn calculate_resource_score(&self) -> f32 {
        // Normalizar recursos a score 0-100
        let cpu_score = (self.resources.cpu_cores / 4.0).min(1.0) * 40.0; // Max 4 cores = 40 puntos
        let memory_score = (self.resources.memory_mb as f32 / 8192.0).min(1.0) * 40.0; // Max 8GB = 40 puntos
        let storage_score = (self.resources.storage_mb as f32 / 10240.0).min(1.0) * 20.0; // Max 10GB = 20 puntos

        let mut total = cpu_score + memory_score + storage_score;

        // Bonus por GPU
        if self.resources.gpu_required {
            total += 20.0;
        }

        total.min(100.0)
    }

    /// Calcula score del tipo de workload
    fn calculate_workload_score(&self) -> f32 {
        let mut score = 0.0;

        // Scripts con interpretes tienen overhead
        if matches!(self.command, CommandType::Script { .. }) {
            score += 10.0;
        }

        // Muchos inputs/outputs = más trabajo
        let io_complexity = self.inputs.len() + self.outputs.len();
        score += (io_complexity as f32 / 10.0).min(1.0) * 20.0;

        // Muchas constraints = scheduling más complejo
        score += (self.constraints.len() as f32 / 5.0).min(1.0) * 10.0;

        score.min(30.0)
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
    state: JobState,
    /// Provider seleccionado (si aplica)
    selected_provider: Option<ProviderId>,
    /// Contexto de ejecución (si el job está en ejecución)
    execution_context: Option<ExecutionContext>,
    /// Número de intentos actuales
    attempts: u32,
    /// Máximo número de intentos
    max_attempts: u32,
    /// Fecha de creación
    created_at: DateTime<Utc>,
    /// Fecha de inicio de ejecución
    started_at: Option<DateTime<Utc>>,
    /// Fecha de finalización
    completed_at: Option<DateTime<Utc>>,
    /// Resultado del job (si completado)
    result: Option<JobResult>,
    /// Mensaje de error (si falló)
    error_message: Option<String>,
    /// Metadatos adicionales
    metadata: HashMap<String, String>,
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

    /// Reconstructs a Job from persistence (Hydration)
    pub fn hydrate(
        id: JobId,
        spec: JobSpec,
        state: JobState,
        selected_provider: Option<ProviderId>,
        execution_context: Option<ExecutionContext>,
        attempts: u32,
        max_attempts: u32,
        created_at: DateTime<Utc>,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
        result: Option<JobResult>,
        error_message: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            id,
            spec,
            state,
            selected_provider,
            execution_context,
            attempts,
            max_attempts,
            created_at,
            started_at,
            completed_at,
            result,
            error_message,
            metadata,
        }
    }

    // Getters
    pub fn state(&self) -> &JobState {
        &self.state
    }
    pub fn selected_provider(&self) -> Option<&ProviderId> {
        self.selected_provider.as_ref()
    }
    pub fn execution_context(&self) -> Option<&ExecutionContext> {
        self.execution_context.as_ref()
    }
    pub fn execution_context_mut(&mut self) -> Option<&mut ExecutionContext> {
        self.execution_context.as_mut()
    }
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }
    pub fn started_at(&self) -> Option<&DateTime<Utc>> {
        self.started_at.as_ref()
    }
    pub fn completed_at(&self) -> Option<&DateTime<Utc>> {
        self.completed_at.as_ref()
    }
    pub fn result(&self) -> Option<&JobResult> {
        self.result.as_ref()
    }
    pub fn result_mut(&mut self) -> Option<&mut JobResult> {
        self.result.as_mut()
    }
    pub fn error_message(&self) -> Option<&String> {
        self.error_message.as_ref()
    }
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    pub fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.metadata
    }

    /// Pone el job en cola
    pub fn queue(&mut self) -> Result<()> {
        match self.state {
            JobState::Pending => Ok(()),
            _ => Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: JobState::Pending,
            }),
        }
    }

    /// Envía el job a un provider (PRD v6.0: Scheduled)
    pub fn submit_to_provider(
        &mut self,
        provider_id: ProviderId,
        context: ExecutionContext,
    ) -> Result<()> {
        let new_state = JobState::Scheduled;

        // Validar transición usando el State Machine
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: new_state,
            });
        }

        self.state = new_state;
        self.selected_provider = Some(provider_id);
        self.execution_context = Some(context);
        self.started_at = Some(Utc::now());
        Ok(())
    }

    /// Asigna el job a un provider pero mantiene estado PENDING
    /// El worker debe confirmar con acknowledgment para cambiar a RUNNING
    pub fn assign_to_provider(
        &mut self,
        provider_id: ProviderId,
        context: ExecutionContext,
    ) -> Result<()> {
        match self.state {
            JobState::Pending | JobState::Assigned => {
                // Asignar provider y contexto
                // Para ASSIGNED (desde atomic dequeue), esto completa la asignación
                self.selected_provider = Some(provider_id);
                self.execution_context = Some(context);
                // No cambiar started_at hasta que worker confirme
                Ok(())
            }
            _ => Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: JobState::Assigned,
            }),
        }
    }

    /// Marca el job como ejecutándose
    pub fn mark_running(&mut self) -> Result<()> {
        // Validar transición usando el State Machine
        let new_state = JobState::Running;
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: new_state,
            });
        }

        self.state = new_state;
        self.started_at = Some(Utc::now());
        Ok(())
    }

    /// Completa el job exitosamente
    pub fn complete(&mut self, result: JobResult) -> Result<()> {
        let new_state = match &result {
            JobResult::Success { .. } => JobState::Succeeded,
            JobResult::Failed { .. } => JobState::Failed,
            JobResult::Cancelled => JobState::Cancelled,
            JobResult::Timeout => JobState::Timeout,
        };

        // Validar transición usando el State Machine
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: new_state,
            });
        }

        self.state = new_state;
        self.completed_at = Some(Utc::now());
        self.result = Some(result);
        Ok(())
    }

    /// Marca el job como fallido
    pub fn fail(&mut self, error_message: String) -> Result<()> {
        let new_state = JobState::Failed;

        // Validar transición usando el State Machine
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: new_state,
            });
        }

        self.state = new_state;
        self.completed_at = Some(Utc::now());
        self.error_message = Some(error_message);
        self.attempts += 1;
        Ok(())
    }

    /// Cancela el job
    pub fn cancel(&mut self) -> Result<()> {
        let new_state = JobState::Cancelled;

        // Validar transición usando el State Machine
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: new_state,
            });
        }

        self.state = new_state;
        self.completed_at = Some(Utc::now());
        Ok(())
    }

    /// Marca el job como timed out
    pub fn timeout(&mut self) -> Result<()> {
        let new_state = JobState::Timeout;

        // Validar transición usando el State Machine
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: new_state,
            });
        }

        self.state = new_state;
        self.completed_at = Some(Utc::now());
        self.result = Some(JobResult::Timeout);
        self.attempts += 1;
        Ok(())
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

    pub fn set_attempts(&mut self, attempts: u32) {
        self.attempts = attempts;
    }

    pub fn set_state(&mut self, new_state: JobState) -> Result<()> {
        // Validar que la transición de estado es válida
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition {
                job_id: self.id.clone(),
                from_state: self.state.clone(),
                to_state: new_state,
            });
        }

        self.state = new_state;
        Ok(())
    }

    /// Determina si el job necesita escalamiento basado en su especificación
    pub fn requires_scaling(&self, queue_depth: usize, threshold: usize) -> bool {
        self.spec.should_escalate(queue_depth, threshold)
    }

    /// Obtiene la prioridad calculada del job
    pub fn calculated_priority(&self) -> JobPriority {
        self.spec.calculate_priority()
    }

    /// Verifica si el job está en un estado terminal
    pub fn is_terminal_state(&self) -> bool {
        matches!(
            self.state,
            JobState::Succeeded | JobState::Failed | JobState::Cancelled | JobState::Timeout
        )
    }

    /// Verifica si el job puede ser cancelado
    pub fn can_be_cancelled(&self) -> bool {
        matches!(
            self.state,
            JobState::Pending | JobState::Scheduled | JobState::Running
        )
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
    // Removed peek() as part of technical debt resolution
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::JobId;

    /// Test: JobSpec validation with valid spec
    #[test]
    fn test_jobspec_validation_valid() {
        let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()])
            .with_timeout(300_000)
            .with_resources(1.0, 512, 1024);

        assert!(spec.validate().is_ok());
    }

    /// Test: JobSpec validation fails with empty command
    #[test]
    fn test_jobspec_validation_empty_command() {
        // Create spec with truly empty command using CommandType directly
        let cmd = CommandType::Shell {
            cmd: "".to_string(),
            args: vec![],
        };
        let spec = JobSpec::with_command(cmd).with_timeout(300_000);
        let result = spec.validate();

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("command"));
        }
    }

    /// Test: JobSpec validation fails with zero timeout
    #[test]
    fn test_jobspec_validation_zero_timeout() {
        let spec = JobSpec::new(vec!["echo".to_string()]).with_timeout(0);
        let result = spec.validate();

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("timeout"));
        }
    }

    /// Test: JobSpec validation fails with zero CPU
    #[test]
    fn test_jobspec_validation_zero_cpu() {
        let spec = JobSpec::new(vec!["echo".to_string()])
            .with_timeout(300_000)
            .with_resources(0.0, 512, 1024);
        let result = spec.validate();

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("cpu_cores"));
        }
    }

    /// Test: JobSpec validation fails with zero memory
    #[test]
    fn test_jobspec_validation_zero_memory() {
        let spec = JobSpec::new(vec!["echo".to_string()])
            .with_timeout(300_000)
            .with_resources(1.0, 0, 1024);
        let result = spec.validate();

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("memory_mb"));
        }
    }

    /// Test: calculate_priority for Critical job
    #[test]
    fn test_calculate_priority_critical() {
        let mut preferences = JobPreferences::default();
        preferences.priority = JobPriority::Critical;

        let spec = JobSpec::new(vec!["echo".to_string()]).with_preferences(preferences);

        assert_eq!(spec.calculate_priority(), JobPriority::Critical);
    }

    /// Test: calculate_priority for high resource job
    #[test]
    fn test_calculate_priority_high_resources() {
        let spec = JobSpec::new(vec!["echo".to_string()])
            .with_resources(4.0, 8192, 10240) // Máximos recursos
            .with_timeout(300_000);

        let priority = spec.calculate_priority();
        assert!(matches!(
            priority,
            JobPriority::High | JobPriority::Critical
        ));
    }

    /// Test: calculate_priority for low resource job
    #[test]
    fn test_calculate_priority_low_resources() {
        let spec = JobSpec::new(vec!["echo".to_string()])
            .with_resources(0.5, 256, 512) // Recursos mínimos
            .with_timeout(300_000);

        let priority = spec.calculate_priority();
        assert!(matches!(priority, JobPriority::Low | JobPriority::Normal));
    }

    /// Test: should_escalate when queue depth exceeds threshold
    #[test]
    fn test_should_escalate_queue_depth() {
        let spec = JobSpec::new(vec!["echo".to_string()]).with_timeout(300_000);

        // Queue depth = threshold
        assert!(spec.should_escalate(10, 10));
        // Queue depth > threshold
        assert!(spec.should_escalate(15, 10));
    }

    /// Test: should_escalate for critical jobs
    #[test]
    fn test_should_escalate_critical_job() {
        let mut preferences = JobPreferences::default();
        preferences.priority = JobPriority::Critical;

        let spec = JobSpec::new(vec!["echo".to_string()])
            .with_preferences(preferences)
            .with_timeout(300_000);

        // Critical jobs always escalate, even with low queue depth
        assert!(spec.should_escalate(1, 10));
    }

    /// Test: should_escalate for high resource jobs with medium queue
    #[test]
    fn test_should_escalate_high_resources_medium_queue() {
        let spec = JobSpec::new(vec!["echo".to_string()])
            .with_resources(4.0, 8192, 10240) // High resources
            .with_timeout(300_000);

        // High resources + queue at half threshold
        assert!(spec.should_escalate(5, 10));
    }

    /// Test: calculate_resource_score for various configurations
    #[test]
    fn test_calculate_resource_score() {
        // Test with maximum resources
        let spec_max = JobSpec::new(vec!["echo".to_string()])
            .with_resources(4.0, 8192, 10240)
            .with_timeout(300_000);
        let score_max = spec_max.calculate_resource_score();
        assert!(score_max >= 80.0 && score_max <= 120.0); // Allow for GPU bonus

        // Test with minimum resources
        let spec_min = JobSpec::new(vec!["echo".to_string()])
            .with_resources(0.5, 256, 512)
            .with_timeout(300_000);
        let score_min = spec_min.calculate_resource_score();
        assert!(score_min < 50.0);
    }

    /// Test: calculate_workload_score for script jobs
    #[test]
    fn test_calculate_workload_score_script() {
        let cmd = CommandType::Script {
            interpreter: "python".to_string(),
            content: "print('hello')".to_string(),
        };

        let spec = JobSpec::with_command(cmd);
        let score = spec.calculate_workload_score();

        // Scripts should have higher score
        assert!(score >= 10.0);
    }

    /// Test: Job::is_terminal_state
    #[test]
    fn test_job_is_terminal_state() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string()]);
        let provider_id = ProviderId::new();

        // Test terminal states - need valid transitions
        let mut job_succeeded = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-1".to_string());
        job_succeeded
            .submit_to_provider(provider_id.clone(), context)
            .unwrap();
        job_succeeded.mark_running().unwrap();
        job_succeeded
            .complete(JobResult::Success {
                exit_code: 0,
                output: "".to_string(),
                error_output: "".to_string(),
            })
            .unwrap();
        assert!(job_succeeded.is_terminal_state());

        let mut job_failed = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-2".to_string());
        job_failed
            .submit_to_provider(provider_id.clone(), context)
            .unwrap();
        job_failed.mark_running().unwrap();
        job_failed
            .complete(JobResult::Failed {
                exit_code: 1,
                error_message: "Job failed".to_string(),
                error_output: "Error".to_string(),
            })
            .unwrap();
        assert!(job_failed.is_terminal_state());

        let mut job_cancelled = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-3".to_string());
        job_cancelled
            .submit_to_provider(provider_id.clone(), context)
            .unwrap();
        job_cancelled.cancel().unwrap();
        assert!(job_cancelled.is_terminal_state());

        let mut job_timeout = Job::new(job_id.clone(), spec.clone());
        job_timeout.set_state(JobState::Timeout).unwrap();
        assert!(job_timeout.is_terminal_state());

        // Test non-terminal states
        let mut job_pending = Job::new(job_id.clone(), spec.clone());
        assert!(!job_pending.is_terminal_state());

        let mut job_running = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-4".to_string());
        job_running
            .submit_to_provider(provider_id.clone(), context)
            .unwrap();
        job_running.mark_running().unwrap();
        assert!(!job_running.is_terminal_state());
    }

    /// Test: Job::can_be_cancelled
    #[test]
    fn test_job_can_be_cancelled() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string()]);
        let provider_id = ProviderId::new();

        // Test cancellable states
        let mut job_pending = Job::new(job_id.clone(), spec.clone());
        // Initial state is Pending, which is cancellable
        assert!(job_pending.can_be_cancelled());

        let mut job_scheduled = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-1".to_string());
        job_scheduled
            .submit_to_provider(provider_id.clone(), context)
            .unwrap();
        assert!(job_scheduled.can_be_cancelled());

        let mut job_running = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-2".to_string());
        job_running
            .submit_to_provider(provider_id.clone(), context)
            .unwrap();
        job_running.mark_running().unwrap();
        assert!(job_running.can_be_cancelled());

        // Test non-cancellable states (terminal states)
        let mut job_succeeded = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-3".to_string());
        job_succeeded
            .submit_to_provider(provider_id.clone(), context)
            .unwrap();
        job_succeeded.mark_running().unwrap();
        job_succeeded
            .complete(JobResult::Success {
                exit_code: 0,
                output: "".to_string(),
                error_output: "".to_string(),
            })
            .unwrap();
        assert!(!job_succeeded.can_be_cancelled());

        let mut job_failed = Job::new(job_id.clone(), spec.clone());
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-4".to_string());
        job_failed.submit_to_provider(provider_id, context).unwrap();
        job_failed.mark_running().unwrap();
        job_failed
            .complete(JobResult::Failed {
                exit_code: 1,
                error_message: "Job failed".to_string(),
                error_output: "Error".to_string(),
            })
            .unwrap();
        assert!(!job_failed.can_be_cancelled());
    }

    /// Test: Job::requires_scaling delegates to spec
    #[test]
    fn test_job_requires_scaling() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string()]).with_timeout(300_000);
        let job = Job::new(job_id, spec);

        // Should delegate to spec.should_escalate
        assert_eq!(
            job.requires_scaling(10, 10),
            job.spec.should_escalate(10, 10)
        );
    }

    /// Test: Job::calculated_priority delegates to spec
    #[test]
    fn test_job_calculated_priority() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string()]).with_timeout(300_000);
        let job = Job::new(job_id, spec);

        // Should delegate to spec.calculate_priority
        assert_eq!(job.calculated_priority(), job.spec.calculate_priority());
    }

    /// Test: Builder Pattern with fluent interface
    #[test]
    fn test_builder_pattern_fluent_interface() {
        let spec = JobSpec::new(vec!["echo".to_string()])
            .with_timeout(600_000)
            .with_resources(2.0, 2048, 4096)
            .with_image("ubuntu:latest".to_string())
            .with_working_dir("/tmp")
            .with_env("KEY".to_string(), "value".to_string());

        assert_eq!(spec.timeout_ms, 600_000);
        assert_eq!(spec.resources.cpu_cores, 2.0);
        assert_eq!(spec.resources.memory_mb, 2048);
        assert_eq!(spec.image, Some("ubuntu:latest".to_string()));
        assert_eq!(spec.working_dir, Some("/tmp".to_string()));
        assert_eq!(spec.env.get("KEY"), Some(&"value".to_string()));
    }

    /// Test: CommandType builder methods
    #[test]
    fn test_command_type_builders() {
        // Test shell builder
        let cmd_shell = CommandType::shell("ls");
        assert!(matches!(cmd_shell, CommandType::Shell { .. }));

        let cmd_shell_args = CommandType::shell_with_args("echo", vec!["hello".to_string()]);
        assert!(matches!(cmd_shell_args, CommandType::Shell { .. }));

        // Test script builder
        let cmd_script = CommandType::script("python", "print('hello')");
        assert!(matches!(cmd_script, CommandType::Script { .. }));

        // Test to_command_vec
        let cmd = CommandType::shell_with_args("echo".to_string(), vec!["hello".to_string()]);
        let vec = cmd.to_command_vec();
        assert_eq!(vec, vec!["echo".to_string(), "hello".to_string()]);
    }

    /// Test: Constraint builder methods
    #[test]
    fn test_constraint_builders() {
        // Test eq constraint
        let constraint = Constraint::eq("key".to_string(), "value".to_string());
        assert!(matches!(constraint.operator, ConstraintOperator::Eq));

        // Test provider constraint
        let provider_constraint = Constraint::provider("docker".to_string());
        assert_eq!(provider_constraint.key, "provider");
        assert!(matches!(
            provider_constraint.operator,
            ConstraintOperator::Eq
        ));
    }

    /// Test: Job::calculate_priority with GPU requirement
    #[test]
    fn test_calculate_priority_with_gpu() {
        // Crear spec con recursos altos
        let spec = JobSpec::new(vec!["echo".to_string()]).with_resources(4.0, 8192, 10240);

        let priority = spec.calculate_priority();
        // Con recursos máximos, debe ser High o Critical
        assert!(matches!(
            priority,
            JobPriority::High | JobPriority::Critical
        ));
    }
}
