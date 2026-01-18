//! Job Executor Service
//!
//! Servicio especializado que ejecuta la acción de enviar RUN_JOB al worker.
//! Implementa el patrón: Saga → Comando → Handler → Servicio (acción gRPC)
//!
//! Este servicio tiene una única responsabilidad: enviar el comando RUN_JOB
//! al worker y publicar eventos via Transactional Outbox.
//!
//! ## Flujo Transactional Outbox:
//! 1. Envía RUN_JOB via gRPC al worker
//! 2. Actualiza estado del job
//! 3. Inserta evento JobStarted en outbox
//! 4. NatsOutboxRelay consume outbox → publica a NATS
//!
//! NOTA: Para garantizar atomicidad completa (update + insert en misma transacción),
//! se requiere que el trait JobRepository incluya update_with_tx.

use crate::workers::commands::WorkerCommandSender;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert, OutboxRepository};
use hodei_server_domain::saga::commands::execution::{ExecuteJobError, JobExecutor};
use hodei_server_domain::shared_kernel::{DomainError, JobId, WorkerId};
use std::sync::Arc;
use tracing::info;

/// Implementación del servicio JobExecutor
///
/// Este servicio delega el envío de RUN_JOB al WorkerCommandSender.
/// Sigue el principio de responsabilidad única (SRP).
pub struct JobExecutorImpl {
    /// Repository para actualizar estado del job
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    /// Sender de comandos gRPC al worker
    worker_command_sender: Arc<dyn WorkerCommandSender + Send + Sync>,
    /// Outbox repository para publicar eventos de forma transaccional
    outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
}

impl JobExecutorImpl {
    /// Crea un nuevo JobExecutorImpl
    pub fn new(
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_command_sender: Arc<dyn WorkerCommandSender + Send + Sync>,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    ) -> Self {
        Self {
            job_repository,
            worker_command_sender,
            outbox_repository,
        }
    }
}

#[async_trait::async_trait]
impl JobExecutor for JobExecutorImpl {
    /// Envía RUN_JOB al worker y publica evento JobStarted via Transactional Outbox
    ///
    /// ## Flujo:
    /// 1. Fetch job del repository
    /// 2. Envía RUN_JOB via gRPC al worker
    /// 3. Actualiza estado del job a RUNNING
    /// 4. Inserta evento JobStarted en outbox
    /// 5. NatsOutboxRelay consume y publica a NATS
    async fn execute_job(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> Result<(), ExecuteJobError> {
        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            "JobExecutorImpl: Starting job execution"
        );

        // Step 1: Fetch job
        let job = self
            .job_repository
            .find_by_id(job_id)
            .await
            .map_err(|e| ExecuteJobError::ExecutionFailed {
                job_id: job_id.clone(),
                source: e.into(),
            })?
            .ok_or_else(|| ExecuteJobError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // Step 2: Send RUN_JOB via gRPC
        self.worker_command_sender
            .send_run_job(worker_id, &job)
            .await
            .map_err(|e| ExecuteJobError::ExecutionFailed {
                job_id: job_id.clone(),
                source: e.into(),
            })?;

        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            "JobExecutorImpl: RUN_JOB sent successfully"
        );

        // Step 3: Update job state to RUNNING
        let mut job_to_update = job.clone();
        job_to_update
            .mark_running()
            .map_err(|e| ExecuteJobError::ExecutionFailed {
                job_id: job_id.clone(),
                source: e.into(),
            })?;

        self.job_repository
            .update(&job_to_update)
            .await
            .map_err(|e| ExecuteJobError::ExecutionFailed {
                job_id: job_id.clone(),
                source: e.into(),
            })?;

        info!(
            job_id = %job_id,
            "JobExecutorImpl: Job state updated to RUNNING"
        );

        // Step 4: Insert JobStarted event to outbox
        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobStarted".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "worker_id": worker_id.0.to_string(),
                "occurred_at": chrono::Utc::now().to_rfc3339()
            }),
            Some(serde_json::json!({
                "source": "JobExecutorImpl",
                "actor": "system:job_executor"
            })),
            None,
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| ExecuteJobError::ExecutionFailed {
                job_id: job_id.clone(),
                source: DomainError::from(OutboxError::from(e)),
            })?;

        info!(
            job_id = %job_id,
            "JobExecutorImpl: JobStarted event inserted to outbox"
        );

        Ok(())
    }
}
