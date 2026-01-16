//! Log Ingestor
//!
//! Componente dedicado para la ingestión y procesamiento de logs de workers.
//! Extraído de WorkerAgentServiceImpl para aplicar Single Responsibility Principle.
//!
//! Responsabilidades:
//! - Recibir logs individuales y batches de workers
//! - Forwardear a LogStreamService para streaming a clientes
//! - Persistir logs a storage (cuando esté configurado)
//! - Manejar finalización de logs por job

use crate::grpc::log_stream::LogStreamService;
use crate::log_persistence::LogStorageRef;
use hodei_jobs::LogEntry;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Resultado de la ingestión de log
#[derive(Debug, Clone)]
pub struct LogIngestionResult {
    pub job_id: String,
    pub lines_ingested: u64,
    pub bytes_processed: usize,
    pub success: bool,
}

/// Configuración del LogIngestor
#[derive(Debug, Clone)]
pub struct LogIngestorConfig {
    /// Tamaño máximo de batch permitido
    pub max_batch_size: usize,
    /// Habilitar persistencia de logs
    pub enable_persistence: bool,
    /// Buffer de logs en memoria (número de líneas)
    pub memory_buffer_lines: usize,
}

impl Default for LogIngestorConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            enable_persistence: false,
            memory_buffer_lines: 10000,
        }
    }
}

/// Interfaz pública del LogIngestor
///
/// Following Clean Architecture, este componente es un "Use Case" que
/// delega la persistencia y streaming a servicios de infraestructura.
#[derive(Clone)]
pub struct LogIngestor {
    /// Servicio de streaming de logs para clientes conectados
    log_service: Option<Arc<LogStreamService>>,
    /// Configuración
    config: LogIngestorConfig,
    /// Métricas
    metrics: Arc<std::sync::Mutex<LogIngestorMetrics>>,
}

impl LogIngestor {
    /// Crear nuevo LogIngestor
    pub fn new(
        log_service: Option<Arc<LogStreamService>>,
        config: Option<LogIngestorConfig>,
    ) -> Self {
        Self {
            log_service,
            config: config.unwrap_or_default(),
            metrics: Arc::new(std::sync::Mutex::new(LogIngestorMetrics::default())),
        }
    }

    /// Ingestar un log individual
    ///
    /// Este método es thread-safe y puede ser llamado concurrentemente
    /// desde múltiples workers.
    pub async fn ingest_log(&self, entry: LogEntry) -> LogIngestionResult {
        let job_id = entry.job_id.clone();
        let line_length = entry.line.len();

        // Forward al servicio de streaming
        if let Some(ref svc) = self.log_service {
            svc.append_log(entry).await;
        } else {
            debug!("No log_service configured, discarding log for job {}", job_id);
        }

        // Actualizar métricas
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_lines += 1;
        metrics.total_bytes += line_length;
        metrics.last_ingestion = Some(chrono::Utc::now());

        LogIngestionResult {
            job_id,
            lines_ingested: 1,
            bytes_processed: line_length,
            success: true,
        }
    }

    /// Ingestar un batch de logs
    ///
    /// Optimizado para reducir overhead en la recepción de múltiples
    /// líneas de log de un worker.
    pub async fn ingest_log_batch(&self, batch: LogBatch) -> LogIngestionResult {
        let job_id = batch.job_id;
        let entry_count = batch.entries.len();
        let bytes: usize = batch.entries.iter().map(|e| e.line.len()).sum();

        // Validar tamaño del batch
        if entry_count > self.config.max_batch_size {
            warn!(
                "Log batch size {} exceeds maximum {}, truncating",
                entry_count,
                self.config.max_batch_size
            );
        }

        // Forward al servicio de streaming
        if let Some(ref svc) = self.log_service {
            for entry in batch.entries.into_iter().take(self.config.max_batch_size) {
                svc.append_log(entry).await;
            }
        }

        // Actualizar métricas
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_batches += 1;
        metrics.total_lines += entry_count as u64;
        metrics.total_bytes += bytes;
        metrics.last_ingestion = Some(chrono::Utc::now());

        LogIngestionResult {
            job_id,
            lines_ingested: entry_count as u64,
            bytes_processed: bytes,
            success: true,
        }
    }

    /// Finalizar logs para un job completado
    ///
    /// Este método se llama cuando un job termina para persistir
    /// los logs acumulados y liberar recursos.
    pub async fn finalize_job_logs(
        &self,
        job_id: &str,
    ) -> Result<LogFinalizationResult, String> {
        if let Some(ref svc) = self.log_service {
            match svc.finalize_job_log(job_id).await {
                Ok(Some(log_ref)) => {
                    info!(
                        "✅ Job {} log finalized and persisted: {} bytes",
                        job_id, log_ref.size_bytes
                    );

                    // Actualizar métricas
                    let mut metrics = self.metrics.lock().unwrap();
                    metrics.jobs_finalized += 1;
                    metrics.total_persisted_bytes += log_ref.size_bytes as usize;

                    Ok(LogFinalizationResult {
                        job_id: job_id.to_string(),
                        persisted: true,
                        size_bytes: log_ref.size_bytes as usize,
                        log_ref: Some(log_ref),
                    })
                }
                Ok(None) => {
                    info!("✅ Job {} completed (no persistent log)", job_id);
                    Ok(LogFinalizationResult {
                        job_id: job_id.to_string(),
                        persisted: false,
                        size_bytes: 0,
                        log_ref: None,
                    })
                }
                Err(e) => {
                    warn!("⚠️ Failed to finalize log for job {}: {}", job_id, e);
                    Err(format!("Failed to finalize logs: {}", e))
                }
            }
        } else {
            // No log service configured
            Ok(LogFinalizationResult {
                job_id: job_id.to_string(),
                persisted: false,
                size_bytes: 0,
                log_ref: None,
            })
        }
    }

    /// Obtener métricas actuales
    pub fn get_metrics(&self) -> LogIngestorMetrics {
        self.metrics.lock().unwrap().clone()
    }
}

/// Batch de logs para ingestión eficiente
#[derive(Debug, Clone)]
pub struct LogBatch {
    pub job_id: String,
    pub entries: Vec<LogEntry>,
}

impl LogBatch {
    /// Crear nuevo batch de logs
    pub fn new(job_id: String, entries: Vec<LogEntry>) -> Self {
        Self { job_id, entries }
    }

    /// Verificar si el batch está vacío
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Obtener número de entradas
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Resultado de la finalizacion de logs de un job
#[derive(Debug, Clone)]
pub struct LogFinalizationResult {
    pub job_id: String,
    pub persisted: bool,
    pub size_bytes: usize,
    pub log_ref: Option<LogStorageRef>,
}

/// Métricas del LogIngestor
#[derive(Debug, Clone, Default)]
pub struct LogIngestorMetrics {
    pub total_lines: u64,
    pub total_batches: u64,
    pub total_bytes: usize,
    pub jobs_finalized: u64,
    pub total_persisted_bytes: usize,
    pub last_ingestion: Option<chrono::DateTime<chrono::Utc>>,
}

impl LogIngestorMetrics {
    /// Crear resumen de métricas
    pub fn summary(&self) -> String {
        format!(
            "LogIngestor: {} lines, {} batches, {} bytes, {} jobs finalized",
            self.total_lines,
            self.total_batches,
            self.total_bytes,
            self.jobs_finalized
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_ingestor_creation() {
        let ingestor = LogIngestor::new(None, None);
        assert!(ingestor.log_service.is_none());
    }

    #[tokio::test]
    async fn test_log_batch_creation() {
        let batch = LogBatch::new(
            "job-123".to_string(),
            vec![
                LogEntry {
                    job_id: "job-123".to_string(),
                    line: "test line 1".to_string(),
                    is_stderr: false,
                    timestamp: None,
                },
                LogEntry {
                    job_id: "job-123".to_string(),
                    line: "test line 2".to_string(),
                    is_stderr: true,
                    timestamp: None,
                },
            ],
        );

        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
    }

    #[tokio::test]
    async fn test_empty_batch() {
        let batch = LogBatch::new("job-123".to_string(), vec![]);
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[tokio::test]
    async fn test_log_ingestion_result() {
        let result = LogIngestionResult {
            job_id: "job-123".to_string(),
            lines_ingested: 5,
            bytes_processed: 100,
            success: true,
        };

        assert_eq!(result.job_id, "job-123");
        assert_eq!(result.lines_ingested, 5);
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_metrics_default() {
        let metrics = LogIngestorMetrics::default();
        assert_eq!(metrics.total_lines, 0);
        assert_eq!(metrics.total_batches, 0);
        assert_eq!(metrics.jobs_finalized, 0);
        assert!(metrics.last_ingestion.is_none());
    }

    #[tokio::test]
    async fn test_metrics_summary() {
        let metrics = LogIngestorMetrics {
            total_lines: 1000,
            total_batches: 50,
            total_bytes: 50000,
            jobs_finalized: 10,
            total_persisted_bytes: 40000,
            last_ingestion: Some(chrono::Utc::now()),
        };

        let summary = metrics.summary();
        assert!(summary.contains("LogIngestor"));
        assert!(summary.contains("1000 lines"));
    }
}
