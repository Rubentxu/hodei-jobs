// Job Execution Coordination Bounded Context
// Coordina la ejecución de jobs y agrega resultados

use crate::jobs::{ExecutionContext, Job, JobRepository};
use crate::shared_kernel::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Resultado de coordinación de ejecución
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionCoordinationResult {
    /// ID del job
    pub job_id: JobId,
    /// ID del provider seleccionado
    pub provider_id: ProviderId,
    /// Contexto de ejecución
    pub execution_context: ExecutionContext,
    /// Fecha de coordinación
    pub coordinated_at: DateTime<Utc>,
    /// Estado de la coordinación
    pub status: CoordinationStatus,
    /// Mensaje de resultado
    pub message: String,
}

/// Estado de la coordinación
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CoordinationStatus {
    /// Coordinación exitosa
    Success,
    /// No hay providers disponibles
    NoProvidersAvailable,
    /// Job ya está ejecutándose
    AlreadyExecuting,
    /// Error de coordinación
    CoordinationError,
    /// Provider seleccionado no puede ejecutar
    ProviderCannotExecute,
}

/// Log entry de ejecución de job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobLogEntry {
    /// ID del job
    pub job_id: JobId,
    /// Timestamp del log
    pub timestamp: DateTime<Utc>,
    /// Nivel del log
    pub level: JobLogLevel,
    /// Mensaje del log
    pub message: String,
    /// Fuente del log (stdout, stderr, system)
    pub source: LogSource,
    /// Línea específica (opcional)
    pub line_number: Option<u32>,
}

impl JobLogEntry {
    pub fn new(job_id: JobId, message: String, timestamp: DateTime<Utc>) -> Self {
        Self {
            job_id,
            timestamp,
            level: JobLogLevel::Info,
            message,
            source: LogSource::Stdout,
            line_number: None,
        }
    }
}

/// Nivel de log de job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobLogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl fmt::Display for JobLogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobLogLevel::Debug => write!(f, "DEBUG"),
            JobLogLevel::Info => write!(f, "INFO"),
            JobLogLevel::Warning => write!(f, "WARNING"),
            JobLogLevel::Error => write!(f, "ERROR"),
            JobLogLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Fuente del log
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogSource {
    Stdout,
    Stderr,
    System,
    Provider(String),
}

/// Resultado agregado de múltiples fuentes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedExecutionResult {
    /// ID del job
    pub job_id: JobId,
    /// Resultado principal
    pub result: JobResult,
    /// Logs agregados
    pub logs: Vec<JobLogEntry>,
    /// Métricas de ejecución
    pub execution_metrics: ExecutionMetrics,
    /// Contexto de ejecución
    pub execution_context: Option<ExecutionContext>,
    /// Fecha de agregación
    pub aggregated_at: DateTime<Utc>,
}

/// Métricas de ejecución
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// Tiempo de cola (ms)
    pub queue_time_ms: u64,
    /// Tiempo de ejecución (ms)
    pub execution_time_ms: u64,
    /// Tiempo total (ms)
    pub total_time_ms: u64,
    /// Memoria peak utilizada (MB)
    pub peak_memory_mb: u64,
    /// CPU promedio utilizado (%)
    pub avg_cpu_usage: f32,
    /// Número de reintentos
    pub retry_count: u32,
    /// Provider utilizado
    pub provider_type: String,
    /// Costo estimado
    pub estimated_cost: Option<f64>,
}

/// Trait para tracking de ejecución
#[async_trait::async_trait]
pub trait ExecutionTracker: Send + Sync {
    async fn track_execution(&self, job_id: &JobId, context: &ExecutionContext) -> Result<()>;
    async fn get_execution_status(&self, job_id: &JobId) -> Result<Option<ExecutionStatus>>;
    async fn update_execution_status(
        &self,
        job_id: &JobId,
        status: ExecutionStatus,
        result: Option<&JobResult>,
    ) -> Result<()>;
    async fn get_execution_context(&self, job_id: &JobId) -> Result<Option<ExecutionContext>>;
    async fn cleanup_old_executions(&self, older_than: Duration) -> Result<u32>;
}

/// Trait para recuperación de logs
#[async_trait::async_trait]
pub trait LogRetriever: Send + Sync {
    async fn get_job_logs(&self, job_id: &JobId) -> Result<Vec<JobLogEntry>>;
    async fn get_logs_since(
        &self,
        job_id: &JobId,
        since: DateTime<Utc>,
    ) -> Result<Vec<JobLogEntry>>;
    async fn add_log_entry(&self, log_entry: JobLogEntry) -> Result<()>;
    async fn clear_job_logs(&self, job_id: &JobId) -> Result<()>;
    async fn get_aggregated_logs(&self, job_id: &JobId) -> Result<Vec<JobLogEntry>>;
}

// NOTE: ExecutionCoordinator será reimplementado en la épica del Scheduler
// usando el nuevo modelo de WorkerProvider y ProviderRegistry

/// Servicio del dominio para agregación de logs
pub struct LogAggregator {
    log_retriever: Box<dyn LogRetriever>,
    job_repository: Box<dyn JobRepository>,
}

impl LogAggregator {
    pub fn new(
        log_retriever: Box<dyn LogRetriever>,
        job_repository: Box<dyn JobRepository>,
    ) -> Self {
        Self {
            log_retriever,
            job_repository,
        }
    }

    /// Agrega logs de múltiples fuentes para un job
    pub async fn aggregate_job_logs(&self, job_id: &JobId) -> Result<AggregatedExecutionResult> {
        // 1. Obtener el job
        let job = self
            .job_repository
            .find_by_id(job_id)
            .await?
            .ok_or_else(|| DomainError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // 2. Obtener logs del job
        let logs = self.log_retriever.get_job_logs(job_id).await?;

        // 3. Calcular métricas de ejecución
        let execution_metrics = self.calculate_execution_metrics(&job, &logs);

        // 4. Crear resultado agregado
        let aggregated_result = AggregatedExecutionResult {
            job_id: job_id.clone(),
            result: job.result().cloned().unwrap_or_else(|| JobResult::Failed {
                exit_code: -1,
                error_message: "No result available".to_string(),
                error_output: String::new(),
            }),
            logs,
            execution_metrics,
            execution_context: job.execution_context().cloned(),
            aggregated_at: Utc::now(),
        };

        Ok(aggregated_result)
    }

    /// Agrega logs para múltiples jobs
    pub async fn aggregate_multiple_job_logs(
        &self,
        job_ids: &[JobId],
    ) -> Result<Vec<AggregatedExecutionResult>> {
        let mut results = Vec::new();

        for job_id in job_ids {
            let result = self.aggregate_job_logs(job_id).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Calcula métricas de ejecución basadas en el job y logs
    fn calculate_execution_metrics(&self, job: &Job, logs: &[JobLogEntry]) -> ExecutionMetrics {
        let queue_time_ms = if let (created, Some(started)) = (job.created_at(), job.started_at()) {
            started.signed_duration_since(*created).num_milliseconds() as u64
        } else {
            0
        };

        let execution_time_ms =
            if let (Some(started), Some(completed)) = (job.started_at(), job.completed_at()) {
                completed.signed_duration_since(*started).num_milliseconds() as u64
            } else {
                0
            };

        let total_time_ms = queue_time_ms + execution_time_ms;

        // Calcular memoria peak y CPU promedio de los logs
        let peak_memory_mb = logs
            .iter()
            .filter_map(|log| {
                // Parse memory usage from log messages
                // This is a simplified implementation
                if log.message.contains("Memory:") {
                    log.message.split_whitespace().find_map(|part| {
                        if part.ends_with("MB") {
                            part[..part.len() - 2].parse::<u64>().ok()
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);

        let avg_cpu_usage = logs
            .iter()
            .filter(|log| log.message.contains("CPU:"))
            .filter_map(|log| {
                log.message.split_whitespace().find_map(|part| {
                    if part.ends_with("%") {
                        part[..part.len() - 1].parse::<f32>().ok()
                    } else {
                        None
                    }
                })
            })
            .sum::<f32>()
            / (logs.len() as f32).max(1.0);

        // Obtener tipo de provider
        let provider_type = job
            .selected_provider()
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        // Estimar costo
        let estimated_cost = job
            .selected_provider()
            .as_ref()
            .and_then(|_| job.spec.preferences.max_budget);

        ExecutionMetrics {
            queue_time_ms,
            execution_time_ms,
            total_time_ms,
            peak_memory_mb,
            avg_cpu_usage,
            retry_count: job.attempts(),
            provider_type,
            estimated_cost,
        }
    }

    /// Limpia logs antiguos
    pub async fn cleanup_old_logs(&self, _older_than: Duration) -> Result<u32> {
        // TODO: Implementar limpieza de logs antiguos
        // Por ahora, retornar contador dummy
        let cleaned_count = 0;

        Ok(cleaned_count)
    }

    /// Busca logs por patrón
    pub async fn search_logs(&self, _pattern: &str) -> Result<Vec<JobLogEntry>> {
        let matching_logs = Vec::new();

        // TODO: Implementar búsqueda en logs
        // Por ahora, retornar lista vacía
        // Esto requeriría un índice de logs o búsqueda en base de datos

        Ok(matching_logs)
    }
}

/// Configuración para el coordinador de ejecución
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionCoordinatorConfig {
    /// Intervalo de coordinación automática (ms)
    pub coordination_interval_ms: u64,
    /// Timeout para coordinación (ms)
    pub coordination_timeout_ms: u64,
    /// Número máximo de coordinaciones concurrentes
    pub max_concurrent_coordinations: usize,
    /// Habilitar coordinación automática
    pub auto_coordination_enabled: bool,
}

impl Default for ExecutionCoordinatorConfig {
    fn default() -> Self {
        Self {
            coordination_interval_ms: 5000, // 5 segundos
            coordination_timeout_ms: 30000, // 30 segundos
            max_concurrent_coordinations: 10,
            auto_coordination_enabled: true,
        }
    }
}
