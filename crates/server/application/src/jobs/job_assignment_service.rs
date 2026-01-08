//! Job Assignment Service
//!
//! Componente dedicado para la asignación de jobs a workers.
//! Extraído de JobDispatcher para aplicar Single Responsibility Principle.
//!
//! Responsabilidades:
//! - Asignar jobs a workers específicos
//! - Enviar comandos RUN_JOB via gRPC
//! - Publicar eventos de asignación
//! - Manejar rollback en caso de fallo

use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::EventMetadata;
use hodei_server_domain::jobs::{ExecutionContext, Job, JobRepository};
use hodei_server_domain::outbox::{OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::{DomainError, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use crate::workers::commands::WorkerCommandSender;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Resultado de la asignación de un job
#[derive(Debug, Clone)]
pub struct JobAssignmentResult {
    /// Si la asignación fue exitosa
    pub success: bool,
    /// Worker al que se asignó el job
    pub worker_id: Option<WorkerId>,
    /// Mensaje de error si falló
    pub error_message: Option<String>,
    /// Tiempo que tomó la asignación
    pub duration_ms: u64,
}

/// Configuración del JobAssignmentService
#[derive(Debug, Clone)]
pub struct JobAssignmentConfig {
    /// Timeout para envío de RUN_JOB
    pub dispatch_timeout: Duration,
    /// Habilitar publicación de eventos
    pub publish_events: bool,
}

impl Default for JobAssignmentConfig {
    fn default() -> Self {
        Self {
            dispatch_timeout: Duration::from_secs(5),
            publish_events: true,
        }
    }
}

/// JobAssignmentService - Componente para asignación de jobs a workers
///
/// Maneja la lógica de asignar un job específico a un worker,
/// incluyendo el envío del comando y la publicación de eventos.
#[derive(Clone)]
pub struct JobAssignmentService {
    /// Repository de jobs
    job_repository: Arc<dyn JobRepository>,
    /// Registry de workers
    worker_registry: Arc<dyn WorkerRegistry>,
    /// Sender de comandos para workers
    worker_command_sender: Arc<dyn WorkerCommandSender>,
    /// Event bus
    event_bus: Arc<dyn EventBus>,
    /// Outbox repository
    outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    /// Configuración
    config: JobAssignmentConfig,
}

impl JobAssignmentService {
    /// Crear nuevo JobAssignmentService
    pub fn new(
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
        event_bus: Arc<dyn EventBus>,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
        config: Option<JobAssignmentConfig>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
            worker_command_sender,
            event_bus,
            outbox_repository,
            config: config.unwrap_or_default(),
        }
    }

    /// Asignar un job a un worker y ejecutar dispatch
    ///
    /// ## Pasos:
    /// 1. Validar worker existe y está disponible
    /// 2. Crear ExecutionContext
    /// 3. Asignar provider al job
    /// 4. Persistir estado en BD
    /// 5. Enviar comando RUN_JOB
    /// 6. Publicar evento JobAssigned
    pub async fn assign_job(
        &self,
        job: &mut Job,
        worker_id: &WorkerId,
    ) -> Result<JobAssignmentResult, String> {
        let start_time = std::time::Instant::now();

        info!(
            job_id = %job.id,
            worker_id = %worker_id,
            "JobAssignmentService: Starting job assignment"
        );

        // Step 1: Get worker details
        let worker = self
            .worker_registry
            .get(worker_id)
            .await
            .map_err(|e| format!("Failed to get worker {}: {}", worker_id, e))?
            .ok_or_else(|| {
                DomainError::WorkerNotFound {
                    worker_id: worker_id.clone(),
                }
            })
            .map_err(|e| format!("Worker not found: {}", e))?;

        debug!(worker_id = %worker_id, "JobAssignmentService: Found worker");

        // Step 2: Create execution context and store provider assignment
        let provider_id = worker.handle().provider_id.clone();
        let context = ExecutionContext::new(
            job.id.clone(),
            provider_id.clone(),
            format!("exec-{}", Uuid::new_v4()),
        );

        // Assign provider to job
        if job.selected_provider().is_none() {
            job.assign_to_provider(provider_id.clone(), context)
                .map_err(|e| format!("Failed to assign provider: {}", e))?;
            info!(
                provider_id = %provider_id,
                job_id = %job.id,
                "JobAssignmentService: Assigned provider to job"
            );
        }

        // Step 3: Update job in repository (BEFORE gRPC to avoid race condition)
        info!(
            job_id = %job.id,
            "JobAssignmentService: Updating job in repository"
        );
        self.job_repository.update(job).await.map_err(|e| {
            format!("Failed to persist job before dispatch: {}", e)
        })?;

        // Step 4: Send RUN_JOB command to worker via gRPC
        info!(
            worker_id = %worker_id,
            job_id = %job.id,
            "JobAssignmentService: Sending RUN_JOB command to worker"
        );

        let dispatch_result = tokio::time::timeout(
            self.config.dispatch_timeout,
            self.worker_command_sender.send_run_job(worker_id, job),
        )
        .await;

        let result = match dispatch_result {
            Ok(Ok(())) => {
                info!(
                    worker_id = %worker_id,
                    job_id = %job.id,
                    "JobAssignmentService: RUN_JOB command sent successfully"
                );
                Ok(JobAssignmentResult {
                    success: true,
                    worker_id: Some(worker_id.clone()),
                    error_message: None,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                })
            }
            Ok(Err(e)) => {
                let error_msg = format!("Failed to send RUN_JOB: {}", e);
                error!(
                    error = %e,
                    worker_id = %worker_id,
                    job_id = %job.id,
                    "JobAssignmentService: Failed to send RUN_JOB"
                );

                // Rollback job state
                self.rollback_job_state(job, &error_msg).await;

                Ok(JobAssignmentResult {
                    success: false,
                    worker_id: None,
                    error_message: Some(error_msg),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                })
            }
            Err(_) => {
                let error_msg = "Timeout waiting for RUN_JOB response".to_string();
                error!(
                    worker_id = %worker_id,
                    job_id = %job.id,
                    "JobAssignmentService: Timeout sending RUN_JOB"
                );

                self.rollback_job_state(job, &error_msg).await;

                Ok(JobAssignmentResult {
                    success: false,
                    worker_id: None,
                    error_message: Some(error_msg),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                })
            }
        };

        // Step 5: Publish JobAssigned event
        if self.config.publish_events {
            self.publish_job_assigned_event(job, worker_id).await;
        }

        result
    }

    /// Rollback job state on dispatch failure
    async fn rollback_job_state(&self, job: &mut Job, error_msg: &str) {
        if let Err(state_err) = job.fail(format!("Dispatch failed: {}", error_msg)) {
            error!(
                error = %state_err,
                job_id = %job.id,
                "JobAssignmentService: Failed to transition job to Failed state"
            );
        } else {
            if let Err(db_err) = self.job_repository.update(job).await {
                error!(
                    error = %db_err,
                    job_id = %job.id,
                    "JobAssignmentService: Failed to persist Failed state to DB"
                );
            } else {
                info!(
                    job_id = %job.id,
                    "JobAssignmentService: Job state rolled back to FAILED"
                );
            }
        }
    }

    /// Publish JobAssigned event
    async fn publish_job_assigned_event(&self, job: &Job, worker_id: &WorkerId) {
        let metadata = EventMetadata::from_job_metadata(job.metadata(), &job.id);
        let idempotency_key = format!("job-assigned-{}-{}", job.id.0, worker_id.0);

        let event = OutboxEventInsert::for_job(
            job.id.0,
            "JobAssigned".to_string(),
            serde_json::json!({
                "job_id": job.id.0.to_string(),
                "worker_id": worker_id.0.to_string(),
                "occurred_at": Utc::now().to_rfc3339()
            }),
            Some(serde_json::json!({
                "source": "JobAssignmentService",
                "correlation_id": metadata.correlation_id,
                "actor": metadata.actor.or(Some("system:job_assignment".to_string()))
            })),
            Some(idempotency_key),
        );

        if let Err(e) = self.outbox_repository.insert_events(&[event]).await {
            error!(
                error = %e,
                job_id = %job.id,
                "JobAssignmentService: Failed to insert JobAssigned event"
            );
        } else {
            debug!(
                job_id = %job.id,
                "JobAssignmentService: JobAssigned event inserted"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_assignment_config_default() {
        let config = JobAssignmentConfig::default();
        assert_eq!(config.dispatch_timeout, Duration::from_secs(5));
        assert!(config.publish_events);
    }

    #[tokio::test]
    async fn test_assignment_result_success() {
        let result = JobAssignmentResult {
            success: true,
            worker_id: Some(WorkerId::new()),
            error_message: None,
            duration_ms: 100,
        };
        assert!(result.success);
        assert!(result.worker_id.is_some());
        assert!(result.error_message.is_none());
    }

    #[tokio::test]
    async fn test_assignment_result_failure() {
        let result = JobAssignmentResult {
            success: false,
            worker_id: None,
            error_message: Some("Timeout".to_string()),
            duration_ms: 5000,
        };
        assert!(!result.success);
        assert!(result.worker_id.is_none());
        assert!(result.error_message.is_some());
    }
}
