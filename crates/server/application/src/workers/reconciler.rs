//! Worker Reconciler
//!
//! Componente dedicado para la reconciliación de estados de workers.
//! Extraído de WorkerLifecycleManager para aplicar Single Responsibility Principle.
//!
//! Responsabilidades:
//! - Detectar workers con heartbeats stale
//! - Emitir eventos de HeartbeatMissed
//! - Manejar reassignación de jobs
//! - Actualizar estados de workers

use hodei_server_domain::{
    event_bus::EventBus,
    outbox::{OutboxEventInsert, OutboxRepository},
    shared_kernel::{JobId, WorkerId, WorkerState},
    workers::WorkerRegistry,
    workers::health::WorkerHealthService,
    workers::Worker,
};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Resultado de la reconciliación
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    /// Workers con heartbeats stale detectados
    pub stale_workers: Vec<WorkerId>,
    /// Workers marcados como unhealthy
    pub workers_marked_unhealthy: Vec<WorkerId>,
    /// Jobs afectados por workers stale
    pub affected_jobs: Vec<JobId>,
    /// Jobs que requieren reassignación
    pub jobs_requiring_reassignment: Vec<JobId>,
}

impl ReconciliationResult {
    /// Verificar si se tomó alguna acción
    pub fn has_changes(&self) -> bool {
        !self.stale_workers.is_empty()
            || !self.workers_marked_unhealthy.is_empty()
            || !self.jobs_requiring_reassignment.is_empty()
    }

    /// Obtener número total de issues detectados
    pub fn total_issues(&self) -> usize {
        self.stale_workers.len() + self.jobs_requiring_reassignment.len()
    }
}

/// Configuración del WorkerReconciler
#[derive(Debug, Clone)]
pub struct WorkerReconcilerConfig {
    /// Timeout para considerar un heartbeat como stale
    pub heartbeat_timeout: Duration,
    /// Grace period antes de considerar un worker como muerto
    pub worker_dead_grace_period: Duration,
}

impl Default for WorkerReconcilerConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout: Duration::from_secs(60),
            worker_dead_grace_period: Duration::from_secs(120),
        }
    }
}

/// WorkerReconciler - Componente para reconciliación de estados de workers
///
/// Usa el patrón Transactional Outbox para consistencia en la publicación
/// de eventos cuando está configurado.
#[derive(Clone)]
pub struct WorkerReconciler {
    /// Registry de workers para operaciones de lectura/escritura
    registry: Arc<dyn WorkerRegistry>,
    /// Servicio de salud para calcular edad de heartbeats
    health_service: Arc<WorkerHealthService>,
    /// Outbox repository para publicación transaccional de eventos
    outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    /// Event bus para publicación directa de eventos
    event_bus: Arc<dyn EventBus>,
    /// Configuración
    config: WorkerReconcilerConfig,
}

impl WorkerReconciler {
    /// Crear nuevo WorkerReconciler
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        health_service: Arc<WorkerHealthService>,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
        event_bus: Arc<dyn EventBus>,
        config: Option<WorkerReconcilerConfig>,
    ) -> Self {
        Self {
            registry,
            health_service,
            outbox_repository,
            event_bus,
            config: config.unwrap_or_default(),
        }
    }

    /// Ejecutar reconciliación de workers
    ///
    /// Este método:
    /// 1. Detecta workers con heartbeats stale
    /// 2. Emite eventos HeartbeatMissed para monitoring
    /// 3. Marca workers como unhealthy si exceden grace period
    /// 4. Trigger job reassignment para jobs afectados
    pub async fn reconcile(&self) -> Result<ReconciliationResult, String> {
        let mut result = ReconciliationResult::default();

        // Encontrar workers con heartbeats stale
        let stale_workers = self
            .registry
            .find_unhealthy(self.config.heartbeat_timeout)
            .await
            .map_err(|e| format!("Failed to find unhealthy workers: {}", e))?;

        for worker in stale_workers {
            let worker_id = worker.id().clone();
            let heartbeat_age = self.health_service.calculate_heartbeat_age(&worker);
            let stale_seconds = heartbeat_age.as_duration().as_secs();

            info!(
                "🔍 Reconciliation: Worker {} has stale heartbeat ({}s ago)",
                worker_id, stale_seconds
            );

            result.stale_workers.push(worker_id.clone());

            // Verificar si el worker tiene un job asignado
            if let Some(job_id) = worker.current_job_id() {
                result.affected_jobs.push(job_id.clone());

                // Emitir eventos para job reassignment
                if let Err(e) = self.emit_heartbeat_missed(&worker, &job_id).await {
                    warn!(
                        "Failed to emit heartbeat missed event for worker {}: {:?}",
                        worker_id, e
                    );
                }

                // Si excede grace period, marcar para reassignment
                if stale_seconds > self.config.worker_dead_grace_period.as_secs() {
                    info!(
                        "⚠️ Worker {} exceeded grace period, triggering job reassignment for {}",
                        worker_id, job_id
                    );

                    if let Err(e) = self.emit_job_reassignment_required(&worker, &job_id).await {
                        warn!(
                            "Failed to emit job reassignment event for job {}: {:?}",
                            job_id, e
                        );
                    }

                    result.jobs_requiring_reassignment.push(job_id.clone());
                }
            }

            // Actualizar estado del worker
            if *worker.state() != WorkerState::Terminated {
                if let Err(e) = self
                    .registry
                    .update_state(&worker_id, WorkerState::Terminated)
                    .await
                {
                    error!(
                        "Failed to update worker {} to Terminated state: {}",
                        worker_id, e
                    );
                } else {
                    result.workers_marked_unhealthy.push(worker_id.clone());

                    // Emitir WorkerStatusChanged event
                    if let Err(e) = self
                        .emit_status_changed(&worker_id, worker.state().clone(), WorkerState::Terminated, "heartbeat_timeout")
                        .await
                    {
                        warn!(
                            "Failed to emit status changed event for worker {}: {:?}",
                            worker_id, e
                        );
                    }
                }
            }
        }

        if !result.stale_workers.is_empty() {
            info!(
                "📊 Reconciliation complete: {} stale workers, {} affected jobs, {} requiring reassignment",
                result.stale_workers.len(),
                result.affected_jobs.len(),
                result.jobs_requiring_reassignment.len()
            );
        }

        Ok(result)
    }

    /// Emitir evento WorkerHeartbeatMissed
    async fn emit_heartbeat_missed(&self, worker: &Worker, job_id: &JobId) -> Result<(), String> {
        let now = Utc::now();
        let worker_id = worker.id();

        let event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerHeartbeatMissed".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "last_heartbeat": worker.updated_at().to_rfc3339(),
                "current_job_id": job_id.0.to_string(),
                "detected_at": now.to_rfc3339()
            }),
            Some(serde_json::json!({
                "source": "WorkerReconciler",
                "reconciliation_run": true
            })),
            Some(format!(
                "heartbeat-missed-{}-{}",
                worker_id.0,
                now.timestamp()
            )),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| format!("Failed to insert outbox event: {:?}", e))?;

        Ok(())
    }

    /// Emitir evento JobReassignmentRequired
    async fn emit_job_reassignment_required(&self, worker: &Worker, job_id: &JobId) -> Result<(), String> {
        let now = Utc::now();
        let worker_id = worker.id();

        let event = OutboxEventInsert::for_job(
            job_id.0,
            "JobReassignmentRequired".to_string(),
            serde_json::json!({
                "job_id": job_id.0.to_string(),
                "failed_worker_id": worker_id.0.to_string(),
                "reason": "worker_heartbeat_timeout",
                "occurred_at": now.to_rfc3339()
            }),
            Some(serde_json::json!({
                "source": "WorkerReconciler",
                "reconciliation_run": true,
                "worker_last_heartbeat": worker.updated_at().to_rfc3339()
            })),
            Some(format!("job-reassign-{}-{}", job_id.0, now.timestamp())),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| format!("Failed to insert outbox event: {:?}", e))?;

        Ok(())
    }

    /// Emitir evento WorkerStatusChanged
    async fn emit_status_changed(
        &self,
        worker_id: &WorkerId,
        old_state: WorkerState,
        new_state: WorkerState,
        reason: &str,
    ) -> Result<(), String> {
        let now = Utc::now();

        let event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerStatusChanged".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "old_status": format!("{:?}", old_state),
                "new_status": format!("{:?}", new_state),
                "reason": reason,
                "occurred_at": now.to_rfc3339()
            }),
            Some(serde_json::json!({
                "source": "WorkerReconciler",
                "transition_reason": reason
            })),
            Some(format!(
                "worker-status-{}-{}-{}",
                worker_id.0,
                format!("{:?}", new_state),
                now.timestamp()
            )),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| format!("Failed to insert outbox event: {:?}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_reconciliation_result_default() {
        let result = ReconciliationResult::default();
        assert!(result.stale_workers.is_empty());
        assert!(!result.has_changes());
        assert_eq!(result.total_issues(), 0);
    }

    #[tokio::test]
    async fn test_reconciliation_result_with_changes() {
        let mut result = ReconciliationResult::default();
        let worker_id = WorkerId::new();
        let job_id = JobId::new();

        result.stale_workers.push(worker_id.clone());
        result.affected_jobs.push(job_id.clone());
        result.jobs_requiring_reassignment.push(job_id.clone());
        result.workers_marked_unhealthy.push(worker_id.clone());

        assert!(result.has_changes());
        assert_eq!(result.total_issues(), 2);
    }

    #[tokio::test]
    async fn test_config_default() {
        let config = WorkerReconcilerConfig::default();
        assert_eq!(config.heartbeat_timeout, Duration::from_secs(60));
        assert_eq!(config.worker_dead_grace_period, Duration::from_secs(120));
    }
}
