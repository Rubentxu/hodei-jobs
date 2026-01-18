use crate::jobs::JobSpec;
use crate::shared_kernel::{JobId, JobState, ProviderId, ProviderStatus, WorkerId, WorkerState};
use crate::workers::ProviderType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Importar tipos de error de hodei_shared
use hodei_shared::states::{
    DispatchFailureReason, JobFailureReason, ProviderErrorType, ProvisioningFailureReason,
    SchedulingFailureReason,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobCreated {
    pub job_id: JobId,
    pub spec: JobSpec,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
}

/// Representa un evento de dominio que ha ocurrido en el sistema.
/// Los eventos son hechos inmutables.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DomainEvent {
    /// Se ha solicitado la creación de un nuevo job
    JobCreated(JobCreated),
    /// El estado de un job ha cambiado
    JobStatusChanged {
        job_id: JobId,
        old_state: JobState,
        new_state: JobState,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un nuevo worker se ha registrado en el sistema
    WorkerRegistered {
        worker_id: WorkerId,
        provider_id: ProviderId,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El estado de un worker ha cambiado
    WorkerStatusChanged {
        worker_id: WorkerId,
        old_status: WorkerState,
        new_status: WorkerState,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un nuevo provider se ha registrado
    ProviderRegistered {
        provider_id: ProviderId,
        provider_type: String,
        config_summary: String, // Resumen de configuración (safe logging)
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un provider ha sido actualizado
    ProviderUpdated {
        provider_id: ProviderId,
        changes: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// La profundidad de la cola de trabajos ha cambiado significativamente
    JobQueueDepthChanged {
        queue_depth: u64,
        threshold: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Se ha disparado el auto-scaling para un provider
    AutoScalingTriggered {
        provider_id: ProviderId,
        reason: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido cancelado explícitamente
    JobCancelled {
        job_id: JobId,
        reason: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker ha sido terminado (desregistrado o destruido)
    WorkerTerminated {
        worker_id: WorkerId,
        provider_id: ProviderId,
        reason: TerminationReason,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker se ha desconectado inesperadamente
    WorkerDisconnected {
        worker_id: WorkerId,
        last_heartbeat: Option<DateTime<Utc>>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker ha sido provisionado por el lifecycle manager
    WorkerProvisioned {
        worker_id: WorkerId,
        provider_id: ProviderId,
        spec_summary: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido reintentado
    JobRetried {
        job_id: JobId,
        attempt: u32,
        max_attempts: u32,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido asignado a un worker específico
    JobAssigned {
        job_id: JobId,
        worker_id: WorkerId,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido aceptado por un worker (ACK recibido)
    JobAccepted {
        job_id: JobId,
        worker_id: WorkerId,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha sido confirmado por el servidor tras recibir ACK del worker
    /// Este evento representa la confirmación de transporte (físico) vs JobAssigned (lógico)
    JobDispatchAcknowledged {
        job_id: JobId,
        worker_id: WorkerId,
        acknowledged_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker ha recibido el comando RUN_JOB (antes del ACK)
    RunJobReceived {
        job_id: JobId,
        worker_id: WorkerId,
        received_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El estado de salud de un provider ha cambiado
    ProviderHealthChanged {
        provider_id: ProviderId,
        old_status: ProviderStatus,
        new_status: ProviderStatus,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker se ha reconectado exitosamente y ha recuperado su sesión
    WorkerReconnected {
        worker_id: WorkerId,
        session_id: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker intentó recuperar su sesión pero falló (sesión expirada o inválida)
    WorkerRecoveryFailed {
        worker_id: WorkerId,
        invalid_session_id: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un provider ha recuperado su salud y está operativo nuevamente
    ProviderRecovered {
        provider_id: ProviderId,
        previous_status: ProviderStatus,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker efímero ha sido creado
    WorkerEphemeralCreated {
        worker_id: WorkerId,
        provider_id: ProviderId,
        max_lifetime_secs: u64,
        ttl_after_completion_secs: Option<u64>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker efímero está listo para recibir jobs
    WorkerEphemeralReady {
        worker_id: WorkerId,
        provider_id: ProviderId,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker está listo para recibir jobs (disponible para asignación)
    WorkerReady {
        worker_id: WorkerId,
        provider_id: ProviderId,
        ready_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Evento compuesto con toda la información del estado del worker
    /// Útil para auditoría y reconciliación - contiene snapshot completo
    WorkerStateUpdated {
        worker_id: WorkerId,
        provider_id: ProviderId,
        old_state: WorkerState,
        new_state: WorkerState,
        current_job_id: Option<JobId>,
        last_heartbeat: Option<DateTime<Utc>>,
        capabilities: Vec<String>,
        metadata: HashMap<String, String>,
        transition_reason: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker efímero ha iniciado su terminación
    WorkerEphemeralTerminating {
        worker_id: WorkerId,
        provider_id: ProviderId,
        reason: TerminationReason,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker efímero ha completado su terminación
    WorkerEphemeralTerminated {
        worker_id: WorkerId,
        provider_id: ProviderId,
        cleanup_scheduled: bool,
        ttl_expires_at: Option<DateTime<Utc>>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker efímero ha sido limpiado por el garbage collector
    WorkerEphemeralCleanedUp {
        worker_id: WorkerId,
        provider_id: ProviderId,
        cleanup_reason: CleanupReason,
        cleanup_duration_ms: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Se ha detectado un worker huérfano
    OrphanWorkerDetected {
        worker_id: WorkerId,
        provider_id: ProviderId,
        last_seen: DateTime<Utc>,
        orphaned_duration_secs: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El garbage collector ha ejecutado un ciclo
    GarbageCollectionCompleted {
        provider_id: ProviderId,
        workers_cleaned: usize,
        orphans_detected: usize,
        errors: usize,
        duration_ms: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un worker efímero ha sido marcado como idle
    WorkerEphemeralIdle {
        worker_id: WorkerId,
        provider_id: ProviderId,
        idle_since: DateTime<Utc>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Un job ha fallado durante la ejecución con información detallada del error
    ///
    /// Este evento se publica cuando un job falla y proporciona información
    /// estructurada sobre la causa del fallo, facilitando el diagnóstico.
    JobExecutionError {
        /// ID del job que falló
        job_id: JobId,
        /// ID del worker que ejecutaba el job
        worker_id: WorkerId,
        /// Razón categorizada del fallo
        failure_reason: JobFailureReason,
        /// Código de salida del proceso
        exit_code: i32,
        /// Comando que se intentaba ejecutar
        command: String,
        /// Argumentos del comando
        arguments: Vec<String>,
        /// Directorio de trabajo
        working_dir: Option<String>,
        /// Tiempo de ejecución en milisegundos
        execution_time_ms: u64,
        /// Acciones sugeridas para resolver el error
        suggested_actions: Vec<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El dispatch de un job al worker ha fallado
    JobDispatchFailed {
        /// ID del job que no pudo ser despachado
        job_id: JobId,
        /// ID del worker destino
        worker_id: WorkerId,
        /// Razón del fallo de dispatch
        failure_reason: DispatchFailureReason,
        /// Número de reintentos realizados
        retry_count: u32,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El provisioning de un worker ha fallado
    WorkerProvisioningError {
        /// ID del worker que no pudo ser provisionado
        worker_id: WorkerId,
        /// ID del provider donde falló el provisioning
        provider_id: ProviderId,
        /// Razón del fallo de provisioning
        failure_reason: ProvisioningFailureReason,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// El scheduler no pudo tomar una decisión para un job
    SchedulingDecisionFailed {
        /// ID del job que no pudo ser programado
        job_id: JobId,
        /// Razón del fallo de scheduling
        failure_reason: SchedulingFailureReason,
        /// Providers que se intentaron
        attempted_providers: Vec<ProviderId>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Error de provider durante la ejecución
    ProviderExecutionError {
        /// ID del provider donde ocurrió el error
        provider_id: ProviderId,
        /// ID del worker afectado
        worker_id: WorkerId,
        /// Tipo de error de provider
        error_type: ProviderErrorType,
        /// Mensaje descriptivo del error
        message: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-29: Job ha sido encolado y espera dispatch reactivo
    JobQueued {
        /// ID del job encolado
        job_id: JobId,
        /// Provider preferido si fue especificado
        preferred_provider: Option<ProviderId>,
        /// Requisitos del job para matching
        job_requirements: JobSpec,
        queued_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-29: Worker está listo para recibir un job assignment
    /// Este evento activa el dispatch reactivo
    WorkerReadyForJob {
        /// ID del worker listo
        worker_id: WorkerId,
        /// ID del provider
        provider_id: ProviderId,
        /// Capacidades del worker para matching
        capabilities: Vec<String>,
        /// Tags del worker para matching
        tags: HashMap<String, String>,
        ready_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-29: Request para provisioning de worker para un job específico
    WorkerProvisioningRequested {
        /// ID del job que necesita worker
        job_id: JobId,
        /// Provider donde crear el worker
        provider_id: ProviderId,
        /// Requisitos del job
        job_requirements: JobSpec,
        requested_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-32: Provider seleccionado para provisioning (trazabilidad de decisiones de scheduling)
    ProviderSelected {
        /// ID del job
        job_id: JobId,
        /// Provider seleccionado
        provider_id: ProviderId,
        /// Tipo del provider
        provider_type: ProviderType,
        /// Estrategia de selección usada
        selection_strategy: String,
        /// Costo efectivo del provider
        effective_cost: f64,
        /// Tiempo de startup efectivo en ms
        effective_startup_ms: u64,
        /// Tiempo que tomó la selección en ms
        elapsed_ms: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-29: Heartbeat del worker (reemplaza polling de monitoreo)
    WorkerHeartbeat {
        /// ID del worker
        worker_id: WorkerId,
        /// Estado actual del worker
        state: WorkerState,
        /// Carga del sistema
        load_average: Option<f64>,
        /// Uso de memoria en MB
        memory_usage_mb: Option<u64>,
        /// ID del job actual si está ejecutando
        current_job_id: Option<JobId>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// Worker decide terminarse a sí mismo tras timeout de cleanup post-job
    WorkerSelfTerminated {
        /// ID del worker que se autoterminó
        worker_id: WorkerId,
        /// ID del provider
        provider_id: ProviderId,
        /// Último job completado (si alguno)
        last_job_id: Option<JobId>,
        /// Tiempo esperado de cleanup
        expected_cleanup_ms: u64,
        /// Tiempo real esperando antes de autoterminarse
        actual_wait_ms: u64,
        /// Estado del worker al momento de autoterminarse
        worker_state: WorkerState,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A new job template has been created
    TemplateCreated {
        /// ID of the template
        template_id: String,
        /// Template name
        template_name: String,
        /// Template version
        version: u32,
        /// User who created it
        created_by: Option<String>,
        /// Summary of the spec
        spec_summary: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A job template has been updated
    TemplateUpdated {
        /// ID of the template
        template_id: String,
        /// Template name
        template_name: String,
        /// Old version
        old_version: u32,
        /// New version
        new_version: u32,
        /// Changes made
        changes: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A job template has been disabled
    TemplateDisabled {
        /// ID of the template
        template_id: String,
        /// Template name
        template_name: String,
        /// Version at time of disabling
        version: u32,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A new run has been created from a template
    TemplateRunCreated {
        /// ID of the template
        template_id: String,
        /// Template name
        template_name: String,
        /// Execution ID
        execution_id: String,
        /// Job ID
        job_id: Option<JobId>,
        /// Job name
        job_name: String,
        /// Version of template used
        template_version: u32,
        /// Execution number
        execution_number: u64,
        /// What triggered this run
        triggered_by: String,
        /// User who triggered (if applicable)
        triggered_by_user: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A job execution has been recorded
    ExecutionRecorded {
        /// ID of the execution
        execution_id: String,
        /// Template ID
        template_id: String,
        /// Job ID
        job_id: Option<JobId>,
        /// Execution status
        status: String,
        /// Exit code (if completed)
        exit_code: Option<i32>,
        /// Duration in ms
        duration_ms: Option<u64>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A scheduled job has been created
    ScheduledJobCreated {
        /// ID of the scheduled job
        scheduled_job_id: String,
        /// Name of the scheduled job
        name: String,
        /// Template ID
        template_id: String,
        /// Cron expression
        cron_expression: String,
        /// Timezone
        timezone: String,
        /// User who created it
        created_by: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A scheduled job has been triggered
    ScheduledJobTriggered {
        /// ID of the scheduled job
        scheduled_job_id: String,
        /// Name of the scheduled job
        name: String,
        /// Template ID
        template_id: String,
        /// Execution ID
        execution_id: String,
        /// Job ID
        job_id: Option<JobId>,
        /// When it was scheduled for
        scheduled_for: DateTime<Utc>,
        /// When it was actually triggered
        triggered_at: DateTime<Utc>,
        /// Parameters used
        parameters: HashMap<String, String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A scheduled job execution was missed
    ScheduledJobMissed {
        /// ID of the scheduled job
        scheduled_job_id: String,
        /// Name of the scheduled job
        name: String,
        /// When it should have run
        scheduled_for: DateTime<Utc>,
        /// When it was detected as missed
        detected_at: DateTime<Utc>,
        /// Reason for missing
        reason: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// EPIC-34: A scheduled job encountered an error
    ScheduledJobError {
        /// ID of the scheduled job
        scheduled_job_id: String,
        /// Name of the scheduled job
        name: String,
        /// Error message
        error_message: String,
        /// Execution ID if available
        execution_id: Option<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
        actor: Option<String>,
    },
    /// GAP-006: A saga has completed successfully
    SagaCompleted {
        /// ID of the saga
        saga_id: Uuid,
        /// Type of saga (e.g., "Execution", "Provisioning", "Recovery")
        saga_type: String,
        /// Duration in milliseconds
        duration_ms: u64,
        /// Number of steps executed
        steps_executed: u32,
        /// Correlation ID for tracing
        correlation_id: Option<String>,
        occurred_at: DateTime<Utc>,
    },
    /// GAP-006: A saga has failed
    SagaFailed {
        /// ID of the saga
        saga_id: Uuid,
        /// Type of saga
        saga_type: String,
        /// Error message describing the failure
        error_message: String,
        /// Step where failure occurred
        failed_at_step: u32,
        /// Whether compensation was triggered
        compensation_triggered: bool,
        /// Correlation ID for tracing
        correlation_id: Option<String>,
        occurred_at: DateTime<Utc>,
    },
    /// GAP-006 & GAP-011: A saga has timed out
    SagaTimedOut {
        /// ID of the saga
        saga_id: Uuid,
        /// Type of saga
        saga_type: String,
        /// Configured timeout duration
        timeout_duration_ms: u64,
        /// Elapsed time before timeout
        elapsed_ms: u64,
        /// Steps completed before timeout
        steps_completed: u32,
        /// Correlation ID for tracing
        correlation_id: Option<String>,
        occurred_at: DateTime<Utc>,
    },
}

/// Razón de limpieza de un worker efímero
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CleanupReason {
    /// Limpieza normal después de terminación exitosa
    NormalTermination,
    /// Limpieza por TTL expirado
    TtlExpired,
    /// Limpieza de worker huérfano detectado
    OrphanDetected,
    /// Limpieza forzada por administrador
    ForceCleanup,
    /// Limpieza por shutdown del provider
    ProviderShutdown,
    /// Limpieza por error en el worker
    WorkerError,
}

impl std::fmt::Display for CleanupReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CleanupReason::NormalTermination => write!(f, "NORMAL_TERMINATION"),
            CleanupReason::TtlExpired => write!(f, "TTL_EXPIRED"),
            CleanupReason::OrphanDetected => write!(f, "ORPHAN_DETECTED"),
            CleanupReason::ForceCleanup => write!(f, "FORCE_CLEANUP"),
            CleanupReason::ProviderShutdown => write!(f, "PROVIDER_SHUTDOWN"),
            CleanupReason::WorkerError => write!(f, "WORKER_ERROR"),
        }
    }
}

/// Razón de terminación de un worker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TerminationReason {
    /// Desregistro voluntario por el worker
    Unregistered,
    /// Terminación por idle timeout
    IdleTimeout,
    /// Terminación por lifetime exceeded
    LifetimeExceeded,
    /// Terminación por health check fallido
    HealthCheckFailed,
    /// Terminación manual por administrador
    ManualTermination,
    /// Terminación por error del provider
    ProviderError { message: String },
    /// EPIC-26 US-26.7: Terminación por TTL after completion excedido
    JobCompleted,
    /// Worker decide terminarse a sí mismo tras timeout de cleanup post-job
    SelfInitiated {
        /// Job que completó antes de la autoterminación
        last_job_id: Option<JobId>,
        /// Tiempo esperado de cleanup
        expected_cleanup_ms: u64,
        /// Tiempo real esperando antes de autoterminarse
        actual_wait_ms: u64,
    },
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminationReason::Unregistered => write!(f, "UNREGISTERED"),
            TerminationReason::IdleTimeout => write!(f, "IDLE_TIMEOUT"),
            TerminationReason::LifetimeExceeded => write!(f, "LIFETIME_EXCEEDED"),
            TerminationReason::HealthCheckFailed => write!(f, "HEALTH_CHECK_FAILED"),
            TerminationReason::ManualTermination => write!(f, "MANUAL_TERMINATION"),
            TerminationReason::ProviderError { message } => {
                write!(f, "PROVIDER_ERROR: {}", message)
            }
            TerminationReason::JobCompleted => write!(f, "JOB_COMPLETED"),
            TerminationReason::SelfInitiated {
                last_job_id,
                expected_cleanup_ms,
                actual_wait_ms,
            } => {
                write!(
                    f,
                    "SELF_INITIATED (job={:?}, expected={}ms, waited={}ms)",
                    last_job_id, expected_cleanup_ms, actual_wait_ms
                )
            }
        }
    }
}

/// Metadatos de auditoría para todos los eventos
///
/// Reduce Connascence of Position transformando los campos de auditoría
/// de una tupla repetitiva a un tipo cohesivo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventMetadata {
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub trace_context: Option<TraceContext>,
}

impl EventMetadata {
    /// Crea nuevos metadatos de evento
    pub fn new(correlation_id: Option<String>, actor: Option<String>) -> Self {
        Self {
            correlation_id,
            actor,
            trace_context: None,
        }
    }

    /// Crea metadatos con correlation_id específico y actor del sistema
    pub fn for_system_event(correlation_id: Option<String>, system_actor: &str) -> Self {
        Self {
            correlation_id,
            actor: Some(system_actor.to_string()),
            trace_context: None,
        }
    }

    /// Crea metadatos desde metadata de un job
    ///
    /// # Algoritmo de extracción
    /// 1. Busca `correlation_id` en metadata del job
    /// 2. Si no existe, usa el job_id como fallback
    /// 3. Busca `actor` en metadata del job
    pub fn from_job_metadata(metadata: &HashMap<String, String>, job_id: &JobId) -> Self {
        Self {
            correlation_id: metadata
                .get("correlation_id")
                .cloned()
                .or_else(|| Some(job_id.to_string())),
            actor: metadata.get("actor").cloned(),
            trace_context: None,
        }
    }

    /// Crea metadatos vacíos
    pub fn empty() -> Self {
        Self {
            correlation_id: None,
            actor: None,
            trace_context: None,
        }
    }

    /// Verifica si los metadatos contienen información de auditoría
    pub fn has_audit_info(&self) -> bool {
        self.correlation_id.is_some() || self.actor.is_some()
    }

    /// Establece el contexto de trace
    pub fn with_trace_context(mut self, trace_context: TraceContext) -> Self {
        self.trace_context = Some(trace_context);
        self
    }

    /// Establece el trace ID y span ID desde strings
    pub fn with_trace_ids(mut self, trace_id: Option<String>, span_id: Option<String>) -> Self {
        self.trace_context = match (trace_id, span_id) {
            (Some(t), Some(s)) => Some(TraceContext {
                trace_id: t,
                span_id: s,
            }),
            _ => None,
        };
        self
    }
}

/// Contexto de trazabilidad OpenTelemetry
///
/// Almacena los identificadores de trace y span para propagar
/// el contexto de trazabilidad a través de servicios.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
}

impl TraceContext {
    /// Crea un nuevo TraceContext
    pub fn new(trace_id: String, span_id: String) -> Self {
        Self { trace_id, span_id }
    }

    /// Crea un TraceContext desde un correlation_id
    /// Genera un UUID como trace_id y un span_id derivado
    pub fn from_correlation_id(correlation_id: &str) -> Self {
        let trace_id = correlation_id.to_string();
        let span_id = format!("span-{}", trace_id);
        Self { trace_id, span_id }
    }

    /// Extrae el trace_id como UUID si es válido
    pub fn trace_id_as_uuid(&self) -> Option<Uuid> {
        Uuid::parse_str(&self.trace_id).ok()
    }

    /// Crea un TraceContext con IDs aleatorios
    pub fn random() -> Self {
        let trace_id = Uuid::new_v4().to_string();
        let span_id = Uuid::new_v4().to_string();
        Self { trace_id, span_id }
    }

    /// Extrae el TraceContext desde headers HTTP
    pub fn from_headers(headers: &impl HeaderExtraction) -> Option<Self> {
        let trace_id = headers
            .get_header("x-trace-id")
            .or_else(|| headers.get_header("traceparent"));
        let span_id = headers
            .get_header("x-span-id")
            .or_else(|| headers.get_header("grpc-trace-bin"));

        match (trace_id, span_id) {
            (Some(trace), Some(span)) => Some(TraceContext::new(trace, span)),
            _ => None,
        }
    }

    /// Extrae el TraceContext desde gRPC metadata
    pub fn from_grpc_metadata(_metadata: &impl std::fmt::Debug) -> Option<Self> {
        // En una implementación completa, extraeríamos de los headers de gRPC
        // Por ahora, retornamos None como placeholder
        None
    }
}

/// Trait para extraer headers de manera polimórfica
pub trait HeaderExtraction {
    /// Obtiene un header por nombre
    fn get_header(&self, name: &str) -> Option<String>;
}

/// Implementación para HashMap de headers
impl HeaderExtraction for std::collections::HashMap<String, String> {
    fn get_header(&self, name: &str) -> Option<String> {
        self.get(name).cloned()
    }
}

/// Implementación para std::collections::BTreeMap
impl HeaderExtraction for std::collections::BTreeMap<String, String> {
    fn get_header(&self, name: &str) -> Option<String> {
        self.get(name).cloned()
    }
}

/// Trait para publicar eventos con metadatos de auditoría
///
/// Centraliza la lógica de auditoría y reduce Connascence of Algorithm
/// al eliminar duplicación de código en múltiples use cases.
pub trait EventPublisher {
    type Error;

    /// Publica un evento con metadatos de auditoría
    ///
    /// # Algoritmo
    /// 1. Enriquecer el evento con los metadatos
    /// 2. Persistir el evento en storage
    /// 3. Notificar a suscriptores
    async fn publish_enriched(
        &self,
        event: DomainEvent,
        metadata: EventMetadata,
    ) -> Result<(), Self::Error>;
}

/// Trait Builder para eventos de dominio
///
/// Proporciona una API fluida para crear eventos con metadatos de auditoría.
/// Reduce Connascence of Position transformando la construcción de eventos
/// de argumentos posicionales a un builder chain.
pub trait EventBuilder {
    type Event;

    /// Construye el evento con los metadatos configurados
    fn build(self) -> Self::Event;

    /// Establece correlation_id
    fn with_correlation_id(self, correlation_id: String) -> Self;

    /// Establece actor
    fn with_actor(self, actor: String) -> Self;
}

/// Builder específico para eventos de JobCreated
pub struct JobCreatedBuilder {
    job_id: JobId,
    spec: JobSpec,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
    actor: Option<String>,
    trace_context: Option<TraceContext>,
}

impl JobCreatedBuilder {
    pub fn new(job_id: JobId, spec: JobSpec) -> Self {
        Self {
            job_id,
            spec,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
            trace_context: None,
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_actor(mut self, actor: String) -> Self {
        self.actor = Some(actor);
        self
    }

    pub fn with_trace_context(mut self, trace_context: TraceContext) -> Self {
        self.trace_context = Some(trace_context);
        self
    }

    pub fn build(self) -> DomainEvent {
        DomainEvent::JobCreated(JobCreated {
            job_id: self.job_id,
            spec: self.spec,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
            actor: self.actor,
        })
    }
}

/// Builder específico para eventos JobStatusChanged
pub struct JobStatusChangedBuilder {
    job_id: JobId,
    old_state: JobState,
    new_state: JobState,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
    actor: Option<String>,
}

impl JobStatusChangedBuilder {
    pub fn new(job_id: JobId, old_state: JobState, new_state: JobState) -> Self {
        Self {
            job_id,
            old_state,
            new_state,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_actor(mut self, actor: String) -> Self {
        self.actor = Some(actor);
        self
    }

    pub fn build(self) -> DomainEvent {
        DomainEvent::JobStatusChanged {
            job_id: self.job_id,
            old_state: self.old_state,
            new_state: self.new_state,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
            actor: self.actor,
        }
    }
}

impl DomainEvent {
    /// Obtiene el ID de correlación asociado al evento (si existe)
    pub fn correlation_id(&self) -> Option<String> {
        match self {
            DomainEvent::JobCreated(JobCreated { correlation_id, .. })
            | DomainEvent::JobStatusChanged { correlation_id, .. }
            | DomainEvent::WorkerRegistered { correlation_id, .. }
            | DomainEvent::WorkerStatusChanged { correlation_id, .. }
            | DomainEvent::ProviderRegistered { correlation_id, .. }
            | DomainEvent::ProviderUpdated { correlation_id, .. }
            | DomainEvent::JobCancelled { correlation_id, .. }
            | DomainEvent::WorkerTerminated { correlation_id, .. }
            | DomainEvent::WorkerDisconnected { correlation_id, .. }
            | DomainEvent::WorkerProvisioned { correlation_id, .. }
            | DomainEvent::JobRetried { correlation_id, .. }
            | DomainEvent::JobAssigned { correlation_id, .. }
            | DomainEvent::JobAccepted { correlation_id, .. }
            | DomainEvent::JobDispatchAcknowledged { correlation_id, .. }
            | DomainEvent::RunJobReceived { correlation_id, .. }
            | DomainEvent::ProviderHealthChanged { correlation_id, .. }
            | DomainEvent::JobQueueDepthChanged { correlation_id, .. }
            | DomainEvent::AutoScalingTriggered { correlation_id, .. }
            | DomainEvent::WorkerReconnected { correlation_id, .. }
            | DomainEvent::WorkerRecoveryFailed { correlation_id, .. }
            | DomainEvent::ProviderRecovered { correlation_id, .. }
            | DomainEvent::WorkerEphemeralCreated { correlation_id, .. }
            | DomainEvent::WorkerEphemeralReady { correlation_id, .. }
            | DomainEvent::WorkerReady { correlation_id, .. }
            | DomainEvent::WorkerStateUpdated { correlation_id, .. }
            | DomainEvent::WorkerEphemeralTerminating { correlation_id, .. }
            | DomainEvent::WorkerEphemeralTerminated { correlation_id, .. }
            | DomainEvent::WorkerEphemeralCleanedUp { correlation_id, .. }
            | DomainEvent::OrphanWorkerDetected { correlation_id, .. }
            | DomainEvent::GarbageCollectionCompleted { correlation_id, .. }
            | DomainEvent::WorkerEphemeralIdle { correlation_id, .. }
            | DomainEvent::JobExecutionError { correlation_id, .. }
            | DomainEvent::JobDispatchFailed { correlation_id, .. }
            | DomainEvent::WorkerProvisioningError { correlation_id, .. }
            | DomainEvent::SchedulingDecisionFailed { correlation_id, .. }
            | DomainEvent::ProviderExecutionError { correlation_id, .. }
            | DomainEvent::JobQueued { correlation_id, .. }
            | DomainEvent::WorkerReadyForJob { correlation_id, .. }
            | DomainEvent::WorkerProvisioningRequested { correlation_id, .. }
            | DomainEvent::ProviderSelected { correlation_id, .. }
            | DomainEvent::WorkerHeartbeat { correlation_id, .. }
            | DomainEvent::WorkerSelfTerminated { correlation_id, .. }
            | DomainEvent::TemplateCreated { correlation_id, .. }
            | DomainEvent::TemplateUpdated { correlation_id, .. }
            | DomainEvent::TemplateDisabled { correlation_id, .. }
            | DomainEvent::TemplateRunCreated { correlation_id, .. }
            | DomainEvent::ExecutionRecorded { correlation_id, .. }
            | DomainEvent::ScheduledJobCreated { correlation_id, .. }
            | DomainEvent::ScheduledJobTriggered { correlation_id, .. }
            | DomainEvent::ScheduledJobMissed { correlation_id, .. }
            | DomainEvent::ScheduledJobError { correlation_id, .. }
            | DomainEvent::SagaCompleted { correlation_id, .. }
            | DomainEvent::SagaFailed { correlation_id, .. }
            | DomainEvent::SagaTimedOut { correlation_id, .. } => correlation_id.clone(),
        }
    }

    /// Obtiene el actor que inició el evento (si existe)
    pub fn actor(&self) -> Option<String> {
        match self {
            DomainEvent::JobCreated(JobCreated { actor, .. })
            | DomainEvent::JobStatusChanged { actor, .. }
            | DomainEvent::WorkerRegistered { actor, .. }
            | DomainEvent::WorkerStatusChanged { actor, .. }
            | DomainEvent::ProviderRegistered { actor, .. }
            | DomainEvent::ProviderUpdated { actor, .. }
            | DomainEvent::JobCancelled { actor, .. }
            | DomainEvent::WorkerTerminated { actor, .. }
            | DomainEvent::WorkerDisconnected { actor, .. }
            | DomainEvent::WorkerProvisioned { actor, .. }
            | DomainEvent::JobRetried { actor, .. }
            | DomainEvent::JobAssigned { actor, .. }
            | DomainEvent::JobAccepted { actor, .. }
            | DomainEvent::JobDispatchAcknowledged { actor, .. }
            | DomainEvent::RunJobReceived { actor, .. }
            | DomainEvent::ProviderHealthChanged { actor, .. }
            | DomainEvent::JobQueueDepthChanged { actor, .. }
            | DomainEvent::AutoScalingTriggered { actor, .. }
            | DomainEvent::WorkerReconnected { actor, .. }
            | DomainEvent::WorkerRecoveryFailed { actor, .. }
            | DomainEvent::ProviderRecovered { actor, .. }
            | DomainEvent::WorkerEphemeralCreated { actor, .. }
            | DomainEvent::WorkerEphemeralReady { actor, .. }
            | DomainEvent::WorkerReady { actor, .. }
            | DomainEvent::WorkerStateUpdated { actor, .. }
            | DomainEvent::WorkerEphemeralTerminating { actor, .. }
            | DomainEvent::WorkerEphemeralTerminated { actor, .. }
            | DomainEvent::WorkerEphemeralCleanedUp { actor, .. }
            | DomainEvent::OrphanWorkerDetected { actor, .. }
            | DomainEvent::GarbageCollectionCompleted { actor, .. }
            | DomainEvent::WorkerEphemeralIdle { actor, .. }
            | DomainEvent::JobExecutionError { actor, .. }
            | DomainEvent::JobDispatchFailed { actor, .. }
            | DomainEvent::WorkerProvisioningError { actor, .. }
            | DomainEvent::SchedulingDecisionFailed { actor, .. }
            | DomainEvent::ProviderExecutionError { actor, .. }
            | DomainEvent::JobQueued { actor, .. }
            | DomainEvent::WorkerReadyForJob { actor, .. }
            | DomainEvent::WorkerProvisioningRequested { actor, .. }
            | DomainEvent::ProviderSelected { actor, .. }
            | DomainEvent::WorkerHeartbeat { actor, .. }
            | DomainEvent::WorkerSelfTerminated { actor, .. }
            | DomainEvent::TemplateCreated { actor, .. }
            | DomainEvent::TemplateUpdated { actor, .. }
            | DomainEvent::TemplateDisabled { actor, .. }
            | DomainEvent::TemplateRunCreated { actor, .. }
            | DomainEvent::ExecutionRecorded { actor, .. }
            | DomainEvent::ScheduledJobCreated { actor, .. }
            | DomainEvent::ScheduledJobTriggered { actor, .. }
            | DomainEvent::ScheduledJobMissed { actor, .. }
            | DomainEvent::ScheduledJobError { actor, .. } => actor.clone(),
            // Saga events don't have actors (internal system events)
            DomainEvent::SagaCompleted { .. }
            | DomainEvent::SagaFailed { .. }
            | DomainEvent::SagaTimedOut { .. } => None,
        }
    }

    /// Obtiene la fecha en que ocurrió el evento
    pub fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            DomainEvent::JobCreated(JobCreated { occurred_at, .. })
            | DomainEvent::JobStatusChanged { occurred_at, .. }
            | DomainEvent::WorkerRegistered { occurred_at, .. }
            | DomainEvent::WorkerStatusChanged { occurred_at, .. }
            | DomainEvent::ProviderRegistered { occurred_at, .. }
            | DomainEvent::ProviderUpdated { occurred_at, .. }
            | DomainEvent::JobCancelled { occurred_at, .. }
            | DomainEvent::WorkerTerminated { occurred_at, .. }
            | DomainEvent::WorkerDisconnected { occurred_at, .. }
            | DomainEvent::WorkerProvisioned { occurred_at, .. }
            | DomainEvent::JobRetried { occurred_at, .. }
            | DomainEvent::JobAssigned { occurred_at, .. }
            | DomainEvent::JobAccepted { occurred_at, .. }
            | DomainEvent::ProviderHealthChanged { occurred_at, .. } => *occurred_at,
            DomainEvent::JobDispatchAcknowledged {
                acknowledged_at, ..
            } => *acknowledged_at,
            DomainEvent::JobQueueDepthChanged { occurred_at, .. }
            | DomainEvent::AutoScalingTriggered { occurred_at, .. }
            | DomainEvent::WorkerReconnected { occurred_at, .. }
            | DomainEvent::WorkerRecoveryFailed { occurred_at, .. }
            | DomainEvent::ProviderRecovered { occurred_at, .. }
            | DomainEvent::WorkerEphemeralCreated { occurred_at, .. }
            | DomainEvent::WorkerEphemeralReady { occurred_at, .. }
            | DomainEvent::WorkerEphemeralTerminating { occurred_at, .. }
            | DomainEvent::WorkerEphemeralTerminated { occurred_at, .. }
            | DomainEvent::WorkerEphemeralCleanedUp { occurred_at, .. }
            | DomainEvent::OrphanWorkerDetected { occurred_at, .. }
            | DomainEvent::GarbageCollectionCompleted { occurred_at, .. }
            | DomainEvent::WorkerEphemeralIdle { occurred_at, .. }
            | DomainEvent::JobExecutionError { occurred_at, .. }
            | DomainEvent::JobDispatchFailed { occurred_at, .. }
            | DomainEvent::WorkerProvisioningError { occurred_at, .. }
            | DomainEvent::SchedulingDecisionFailed { occurred_at, .. }
            | DomainEvent::ProviderExecutionError { occurred_at, .. }
            | DomainEvent::WorkerHeartbeat { occurred_at, .. } => *occurred_at,

            DomainEvent::JobQueued { queued_at, .. } => *queued_at,
            DomainEvent::WorkerProvisioningRequested { requested_at, .. } => *requested_at,
            DomainEvent::ProviderSelected { occurred_at, .. } => *occurred_at,
            DomainEvent::WorkerReadyForJob { ready_at, .. } => *ready_at,

            DomainEvent::RunJobReceived { received_at, .. } => *received_at,
            DomainEvent::WorkerReady { ready_at, .. } => *ready_at,
            DomainEvent::WorkerStateUpdated { occurred_at, .. } => *occurred_at,
            DomainEvent::WorkerSelfTerminated { occurred_at, .. } => *occurred_at,
            // EPIC-34: Template events
            DomainEvent::TemplateCreated { occurred_at, .. } => *occurred_at,
            DomainEvent::TemplateUpdated { occurred_at, .. } => *occurred_at,
            DomainEvent::TemplateDisabled { occurred_at, .. } => *occurred_at,
            DomainEvent::TemplateRunCreated { occurred_at, .. } => *occurred_at,
            DomainEvent::ExecutionRecorded { occurred_at, .. } => *occurred_at,
            // EPIC-34: Scheduling events
            DomainEvent::ScheduledJobCreated { occurred_at, .. } => *occurred_at,
            DomainEvent::ScheduledJobTriggered { occurred_at, .. } => *occurred_at,
            DomainEvent::ScheduledJobMissed { occurred_at, .. } => *occurred_at,
            DomainEvent::ScheduledJobError { occurred_at, .. } => *occurred_at,
            // GAP-006: Saga events
            DomainEvent::SagaCompleted { occurred_at, .. } => *occurred_at,
            DomainEvent::SagaFailed { occurred_at, .. } => *occurred_at,
            DomainEvent::SagaTimedOut { occurred_at, .. } => *occurred_at,
        }
    }

    /// Obtiene el tipo de evento como string
    pub fn event_type(&self) -> &'static str {
        match self {
            DomainEvent::JobCreated(JobCreated { .. }) => "JobCreated",
            DomainEvent::JobStatusChanged { .. } => "JobStatusChanged",
            DomainEvent::WorkerRegistered { .. } => "WorkerRegistered",
            DomainEvent::WorkerStatusChanged { .. } => "WorkerStatusChanged",
            DomainEvent::WorkerStateUpdated { .. } => "WorkerStateUpdated",
            DomainEvent::ProviderRegistered { .. } => "ProviderRegistered",
            DomainEvent::ProviderUpdated { .. } => "ProviderUpdated",

            DomainEvent::JobCancelled { .. } => "JobCancelled",
            DomainEvent::WorkerTerminated { .. } => "WorkerTerminated",
            DomainEvent::WorkerDisconnected { .. } => "WorkerDisconnected",
            DomainEvent::WorkerProvisioned { .. } => "WorkerProvisioned",
            DomainEvent::JobRetried { .. } => "JobRetried",
            DomainEvent::JobAssigned { .. } => "JobAssigned",
            DomainEvent::JobAccepted { .. } => "JobAccepted",
            DomainEvent::JobDispatchAcknowledged { .. } => "JobDispatchAcknowledged",
            DomainEvent::RunJobReceived { .. } => "RunJobReceived",
            DomainEvent::ProviderHealthChanged { .. } => "ProviderHealthChanged",
            DomainEvent::JobQueueDepthChanged { .. } => "JobQueueDepthChanged",
            DomainEvent::AutoScalingTriggered { .. } => "AutoScalingTriggered",
            DomainEvent::WorkerReconnected { .. } => "WorkerReconnected",
            DomainEvent::WorkerRecoveryFailed { .. } => "WorkerRecoveryFailed",
            DomainEvent::ProviderRecovered { .. } => "ProviderRecovered",
            DomainEvent::WorkerEphemeralCreated { .. } => "WorkerEphemeralCreated",
            DomainEvent::WorkerEphemeralReady { .. } => "WorkerEphemeralReady",
            DomainEvent::WorkerReady { .. } => "WorkerReady",
            DomainEvent::WorkerEphemeralTerminating { .. } => "WorkerEphemeralTerminating",
            DomainEvent::WorkerEphemeralTerminated { .. } => "WorkerEphemeralTerminated",
            DomainEvent::WorkerEphemeralCleanedUp { .. } => "WorkerEphemeralCleanedUp",
            DomainEvent::OrphanWorkerDetected { .. } => "OrphanWorkerDetected",
            DomainEvent::GarbageCollectionCompleted { .. } => "GarbageCollectionCompleted",
            DomainEvent::WorkerEphemeralIdle { .. } => "WorkerEphemeralIdle",
            DomainEvent::JobExecutionError { .. } => "JobExecutionError",
            DomainEvent::JobDispatchFailed { .. } => "JobDispatchFailed",
            DomainEvent::WorkerProvisioningError { .. } => "WorkerProvisioningError",
            DomainEvent::SchedulingDecisionFailed { .. } => "SchedulingDecisionFailed",
            DomainEvent::ProviderExecutionError { .. } => "ProviderExecutionError",
            // EPIC-29: Reactive events
            DomainEvent::JobQueued { .. } => "JobQueued",
            DomainEvent::WorkerReadyForJob { .. } => "WorkerReadyForJob",
            DomainEvent::WorkerProvisioningRequested { .. } => "WorkerProvisioningRequested",
            DomainEvent::ProviderSelected { .. } => "ProviderSelected",
            DomainEvent::WorkerHeartbeat { .. } => "WorkerHeartbeat",
            // Self-termination event
            DomainEvent::WorkerSelfTerminated { .. } => "WorkerSelfTerminated",
            // EPIC-34: Template events
            DomainEvent::TemplateCreated { .. } => "TemplateCreated",
            DomainEvent::TemplateUpdated { .. } => "TemplateUpdated",
            DomainEvent::TemplateDisabled { .. } => "TemplateDisabled",
            DomainEvent::TemplateRunCreated { .. } => "TemplateRunCreated",
            DomainEvent::ExecutionRecorded { .. } => "ExecutionRecorded",
            // EPIC-34: Scheduling events
            DomainEvent::ScheduledJobCreated { .. } => "ScheduledJobCreated",
            DomainEvent::ScheduledJobTriggered { .. } => "ScheduledJobTriggered",
            DomainEvent::ScheduledJobMissed { .. } => "ScheduledJobMissed",
            DomainEvent::ScheduledJobError { .. } => "ScheduledJobError",
            // GAP-006: Saga events
            DomainEvent::SagaCompleted { .. } => "SagaCompleted",
            DomainEvent::SagaFailed { .. } => "SagaFailed",
            DomainEvent::SagaTimedOut { .. } => "SagaTimedOut",
        }
    }

    /// Obtiene el ID del agregado principal asociado al evento
    pub fn aggregate_id(&self) -> String {
        match self {
            DomainEvent::JobCreated(JobCreated { job_id, .. }) => job_id.to_string(),
            DomainEvent::JobStatusChanged { job_id, .. } => job_id.to_string(),
            DomainEvent::JobCancelled { job_id, .. } => job_id.to_string(),
            DomainEvent::JobRetried { job_id, .. } => job_id.to_string(),
            DomainEvent::JobAssigned { job_id, .. } => job_id.to_string(),
            DomainEvent::JobAccepted { job_id, .. } => job_id.to_string(),
            DomainEvent::JobDispatchAcknowledged { job_id, .. } => job_id.to_string(),
            DomainEvent::RunJobReceived { job_id, .. } => job_id.to_string(),

            DomainEvent::WorkerRegistered { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerStatusChanged { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerTerminated { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerDisconnected { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerProvisioned { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerReconnected { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerRecoveryFailed { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerEphemeralCreated { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerEphemeralReady { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerReady { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerStateUpdated { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerEphemeralTerminating { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerEphemeralTerminated { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerEphemeralCleanedUp { worker_id, .. } => worker_id.to_string(),
            DomainEvent::OrphanWorkerDetected { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerEphemeralIdle { worker_id, .. } => worker_id.to_string(),

            DomainEvent::GarbageCollectionCompleted { provider_id, .. } => provider_id.to_string(),

            DomainEvent::ProviderRegistered { provider_id, .. } => provider_id.to_string(),
            DomainEvent::ProviderUpdated { provider_id, .. } => provider_id.to_string(),
            DomainEvent::ProviderHealthChanged { provider_id, .. } => provider_id.to_string(),
            DomainEvent::AutoScalingTriggered { provider_id, .. } => provider_id.to_string(),
            DomainEvent::ProviderRecovered { provider_id, .. } => provider_id.to_string(),

            // Para eventos globales o de sistema sin ID específico claro, usamos "SYSTEM" o el valor más relevante
            DomainEvent::JobQueueDepthChanged { .. } => "SYSTEM_QUEUE".to_string(),

            // Nuevos eventos de error
            DomainEvent::JobExecutionError { job_id, .. } => job_id.to_string(),
            DomainEvent::JobDispatchFailed { job_id, .. } => job_id.to_string(),
            DomainEvent::WorkerProvisioningError { worker_id, .. } => worker_id.to_string(),
            DomainEvent::SchedulingDecisionFailed { job_id, .. } => job_id.to_string(),
            DomainEvent::ProviderExecutionError { provider_id, .. } => provider_id.to_string(),

            // EPIC-29: Reactive events
            DomainEvent::JobQueued { job_id, .. } => job_id.to_string(),
            DomainEvent::WorkerReadyForJob { worker_id, .. } => worker_id.to_string(),
            DomainEvent::WorkerProvisioningRequested { job_id, .. } => job_id.to_string(),
            DomainEvent::ProviderSelected { job_id, .. } => job_id.to_string(),
            DomainEvent::WorkerHeartbeat { worker_id, .. } => worker_id.to_string(),
            // Self-termination event
            DomainEvent::WorkerSelfTerminated { worker_id, .. } => worker_id.to_string(),
            // EPIC-34: Template events
            DomainEvent::TemplateCreated { template_id, .. } => template_id.clone(),
            DomainEvent::TemplateUpdated { template_id, .. } => template_id.clone(),
            DomainEvent::TemplateDisabled { template_id, .. } => template_id.clone(),
            DomainEvent::TemplateRunCreated { template_id, .. } => template_id.clone(),
            DomainEvent::ExecutionRecorded { template_id, .. } => template_id.clone(),
            // EPIC-34: Scheduling events
            DomainEvent::ScheduledJobCreated {
                scheduled_job_id, ..
            } => scheduled_job_id.clone(),
            DomainEvent::ScheduledJobTriggered {
                scheduled_job_id, ..
            } => scheduled_job_id.clone(),
            DomainEvent::ScheduledJobMissed {
                scheduled_job_id, ..
            } => scheduled_job_id.clone(),
            DomainEvent::ScheduledJobError {
                scheduled_job_id, ..
            } => scheduled_job_id.clone(),
            // GAP-006: Saga events - aggregate ID is the saga ID
            DomainEvent::SagaCompleted { saga_id, .. } => saga_id.to_string(),
            DomainEvent::SagaFailed { saga_id, .. } => saga_id.to_string(),
            DomainEvent::SagaTimedOut { saga_id, .. } => saga_id.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);
        let event = DomainEvent::JobCreated(JobCreated {
            job_id: job_id.clone(),
            spec: spec.clone(),
            occurred_at: Utc::now(),
            correlation_id: Some("test-correlation-id".to_string()),
            actor: Some("test-actor".to_string()),
        });

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_worker_terminated_event_serialization() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let event = DomainEvent::WorkerTerminated {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            reason: TerminationReason::IdleTimeout,
            occurred_at: Utc::now(),
            correlation_id: Some("test-correlation".to_string()),
            actor: Some("system".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "WorkerTerminated");
    }

    #[test]
    fn test_worker_disconnected_event_serialization() {
        let worker_id = WorkerId::new();
        let event = DomainEvent::WorkerDisconnected {
            worker_id: worker_id.clone(),
            last_heartbeat: Some(Utc::now()),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "WorkerDisconnected");
    }

    #[test]
    fn test_worker_provisioned_event_serialization() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let event = DomainEvent::WorkerProvisioned {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            spec_summary: "image=hodei-worker:latest, cpu=2, memory=4Gi".to_string(),
            occurred_at: Utc::now(),
            correlation_id: Some("provision-123".to_string()),
            actor: Some("scheduler".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "WorkerProvisioned");
    }

    #[test]
    fn test_termination_reason_display() {
        assert_eq!(TerminationReason::Unregistered.to_string(), "UNREGISTERED");
        assert_eq!(TerminationReason::IdleTimeout.to_string(), "IDLE_TIMEOUT");
        assert_eq!(
            TerminationReason::LifetimeExceeded.to_string(),
            "LIFETIME_EXCEEDED"
        );
        assert_eq!(
            TerminationReason::HealthCheckFailed.to_string(),
            "HEALTH_CHECK_FAILED"
        );
        assert_eq!(
            TerminationReason::ManualTermination.to_string(),
            "MANUAL_TERMINATION"
        );
        assert_eq!(
            TerminationReason::ProviderError {
                message: "connection lost".to_string()
            }
            .to_string(),
            "PROVIDER_ERROR: connection lost"
        );
    }

    #[test]
    fn test_event_type_method() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string()]);

        let events = vec![
            (
                DomainEvent::JobCreated(JobCreated {
                    job_id: job_id.clone(),
                    spec,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                }),
                "JobCreated",
            ),
            (
                DomainEvent::WorkerTerminated {
                    worker_id: WorkerId::new(),
                    provider_id: ProviderId::new(),
                    reason: TerminationReason::Unregistered,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "WorkerTerminated",
            ),
            (
                DomainEvent::WorkerDisconnected {
                    worker_id: WorkerId::new(),
                    last_heartbeat: None,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "WorkerDisconnected",
            ),
            (
                DomainEvent::WorkerProvisioned {
                    worker_id: WorkerId::new(),
                    provider_id: ProviderId::new(),
                    spec_summary: "test".to_string(),
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "WorkerProvisioned",
            ),
            (
                DomainEvent::JobRetried {
                    job_id: JobId::new(),
                    attempt: 2,
                    max_attempts: 3,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "JobRetried",
            ),
            (
                DomainEvent::JobAssigned {
                    job_id: JobId::new(),
                    worker_id: WorkerId::new(),
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "JobAssigned",
            ),
            (
                DomainEvent::RunJobReceived {
                    job_id: JobId::new(),
                    worker_id: WorkerId::new(),
                    received_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "RunJobReceived",
            ),
            (
                DomainEvent::WorkerReady {
                    worker_id: WorkerId::new(),
                    provider_id: ProviderId::new(),
                    ready_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                },
                "WorkerReady",
            ),
        ];

        for (event, expected_type) in events {
            assert_eq!(event.event_type(), expected_type);
        }
    }

    #[test]
    fn test_job_retried_event_serialization() {
        let job_id = JobId::new();
        let event = DomainEvent::JobRetried {
            job_id: job_id.clone(),
            attempt: 2,
            max_attempts: 3,
            occurred_at: Utc::now(),
            correlation_id: Some("retry-123".to_string()),
            actor: Some("scheduler".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "JobRetried");
    }

    #[test]
    fn test_job_assigned_event_serialization() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let event = DomainEvent::JobAssigned {
            job_id: job_id.clone(),
            worker_id: worker_id.clone(),
            occurred_at: Utc::now(),
            correlation_id: Some("assign-456".to_string()),
            actor: Some("job-controller".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "JobAssigned");
    }

    #[test]
    fn test_run_job_received_event_serialization() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let event = DomainEvent::RunJobReceived {
            job_id: job_id.clone(),
            worker_id: worker_id.clone(),
            received_at: Utc::now(),
            correlation_id: Some("run-789".to_string()),
            actor: Some("worker-agent".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "RunJobReceived");
    }

    #[test]
    fn test_worker_ready_event_serialization() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let event = DomainEvent::WorkerReady {
            worker_id: worker_id.clone(),
            provider_id: provider_id.clone(),
            ready_at: Utc::now(),
            correlation_id: Some("ready-123".to_string()),
            actor: Some("worker-lifecycle".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "WorkerReady");
    }

    #[test]
    fn test_provider_health_changed_event_serialization() {
        let provider_id = ProviderId::new();
        let event = DomainEvent::ProviderHealthChanged {
            provider_id: provider_id.clone(),
            old_status: ProviderStatus::Active,
            new_status: ProviderStatus::Unhealthy,
            occurred_at: Utc::now(),
            correlation_id: Some("health-check-789".to_string()),
            actor: Some("health-monitor".to_string()),
        };

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(event, deserialized);
        assert_eq!(event.event_type(), "ProviderHealthChanged");
    }
    #[test]
    fn test_worker_reconnection_events_serialization() {
        let worker_id = WorkerId::new();
        let reconnected = DomainEvent::WorkerReconnected {
            worker_id: worker_id.clone(),
            session_id: "sess-123".to_string(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let serialized = serde_json::to_string(&reconnected).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(reconnected, deserialized);
        assert_eq!(reconnected.event_type(), "WorkerReconnected");

        let failed = DomainEvent::WorkerRecoveryFailed {
            worker_id: worker_id.clone(),
            invalid_session_id: "bad-sess".to_string(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let serialized = serde_json::to_string(&failed).expect("Failed to serialize");
        let deserialized: DomainEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(failed, deserialized);
        assert_eq!(failed.event_type(), "WorkerRecoveryFailed");
    }

    // ========================================================================
    // EventMetadata Tests
    // ========================================================================

    #[test]
    fn test_event_metadata_creation() {
        let metadata =
            EventMetadata::new(Some("corr-123".to_string()), Some("user-456".to_string()));

        assert_eq!(metadata.correlation_id, Some("corr-123".to_string()));
        assert_eq!(metadata.actor, Some("user-456".to_string()));
        assert!(metadata.has_audit_info());
    }

    #[test]
    fn test_event_metadata_from_job_metadata_with_correlation() {
        let job_id = JobId::new();
        let mut metadata = HashMap::new();
        metadata.insert("correlation_id".to_string(), "workflow-789".to_string());
        metadata.insert("actor".to_string(), "scheduler".to_string());

        let event_metadata = EventMetadata::from_job_metadata(&metadata, &job_id);

        assert_eq!(
            event_metadata.correlation_id,
            Some("workflow-789".to_string())
        );
        assert_eq!(event_metadata.actor, Some("scheduler".to_string()));
    }

    #[test]
    fn test_event_metadata_from_job_metadata_fallback_to_job_id() {
        let job_id = JobId::new();
        let metadata = HashMap::new();

        let event_metadata = EventMetadata::from_job_metadata(&metadata, &job_id);

        // Should fallback to job_id when correlation_id is not present
        assert_eq!(event_metadata.correlation_id, Some(job_id.to_string()));
        assert_eq!(event_metadata.actor, None);
    }

    #[test]
    fn test_event_metadata_for_system_event() {
        let event_metadata = EventMetadata::for_system_event(
            Some("system-corr".to_string()),
            "system:worker_monitor",
        );

        assert_eq!(
            event_metadata.correlation_id,
            Some("system-corr".to_string())
        );
        assert_eq!(
            event_metadata.actor,
            Some("system:worker_monitor".to_string())
        );
    }

    #[test]
    fn test_event_metadata_empty() {
        let event_metadata = EventMetadata::empty();

        assert_eq!(event_metadata.correlation_id, None);
        assert_eq!(event_metadata.actor, None);
        assert!(!event_metadata.has_audit_info());
    }

    #[test]
    fn test_event_metadata_serialization() {
        let original = EventMetadata::new(
            Some("test-correlation".to_string()),
            Some("test-actor".to_string()),
        );

        let serialized = serde_json::to_string(&original).expect("Failed to serialize");
        let deserialized: EventMetadata =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_event_metadata_partial_info() {
        // Test with only correlation_id
        let metadata1 = EventMetadata::new(Some("corr".to_string()), None);
        assert!(metadata1.has_audit_info());

        // Test with only actor
        let metadata2 = EventMetadata::new(None, Some("actor".to_string()));
        assert!(metadata2.has_audit_info());

        // Test with neither
        let metadata3 = EventMetadata::empty();
        assert!(!metadata3.has_audit_info());
    }

    // ========================================================================
    // EventPublisher Tests
    // ========================================================================
    //
    // Note: Async trait tests are implemented in integration tests
    // since this module is synchronous. See hodei-server-integration crate.

    // ========================================================================
    // EventBuilder Tests
    // ========================================================================

    #[test]
    fn test_job_created_builder() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["test".to_string(), "command".to_string()]);

        let event = JobCreatedBuilder::new(job_id.clone(), spec.clone())
            .with_correlation_id("workflow-456".to_string())
            .with_actor("system:builder".to_string())
            .build();

        match event {
            DomainEvent::JobCreated(JobCreated {
                job_id: actual_job_id,
                spec: actual_spec,
                occurred_at: _,
                correlation_id,
                actor,
            }) => {
                assert_eq!(actual_job_id, job_id);
                assert_eq!(actual_spec, spec);
                assert_eq!(correlation_id, Some("workflow-456".to_string()));
                assert_eq!(actor, Some("system:builder".to_string()));
            }
            _ => panic!("Expected JobCreated event"),
        }
    }

    #[test]
    fn test_job_status_changed_builder() {
        use crate::shared_kernel::JobState;

        let job_id = JobId::new();
        let old_state = JobState::Pending;
        let new_state = JobState::Running;

        let event =
            JobStatusChangedBuilder::new(job_id.clone(), old_state.clone(), new_state.clone())
                .with_correlation_id("transition-789".to_string())
                .with_actor("system:dispatcher".to_string())
                .build();

        match event {
            DomainEvent::JobStatusChanged {
                job_id: actual_job_id,
                old_state: actual_old_state,
                new_state: actual_new_state,
                occurred_at: _,
                correlation_id,
                actor,
            } => {
                assert_eq!(actual_job_id, job_id);
                assert_eq!(actual_old_state, old_state);
                assert_eq!(actual_new_state, new_state);
                assert_eq!(correlation_id, Some("transition-789".to_string()));
                assert_eq!(actor, Some("system:dispatcher".to_string()));
            }
            _ => panic!("Expected JobStatusChanged event"),
        }
    }

    #[test]
    fn test_job_created_builder_without_metadata() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["test".to_string()]);

        let event = JobCreatedBuilder::new(job_id.clone(), spec.clone()).build();

        match event {
            DomainEvent::JobCreated(JobCreated {
                job_id: actual_job_id,
                spec: actual_spec,
                occurred_at: _,
                correlation_id,
                actor,
            }) => {
                assert_eq!(actual_job_id, job_id);
                assert_eq!(actual_spec, spec);
                assert_eq!(correlation_id, None);
                assert_eq!(actor, None);
            }
            _ => panic!("Expected JobCreated event"),
        }
    }

    #[test]
    fn test_builder_fluent_api() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);

        // Test fluent API chaining
        let event = JobCreatedBuilder::new(job_id.clone(), spec.clone())
            .with_correlation_id("chain-test".to_string())
            .with_actor("test-user".to_string())
            .with_correlation_id("override".to_string()) // Should override
            .build();

        match event {
            DomainEvent::JobCreated(JobCreated {
                correlation_id,
                actor,
                ..
            }) => {
                assert_eq!(correlation_id, Some("override".to_string()));
                assert_eq!(actor, Some("test-user".to_string()));
            }
            _ => panic!("Expected JobCreated event"),
        }
    }
}
