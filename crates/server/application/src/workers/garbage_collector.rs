//! Worker Garbage Collector
//!
//! Componente dedicado para la limpieza de workers terminated/orphaned.
//! Extraído de WorkerLifecycleManager para aplicar Single Responsibility Principle.
//!
//! Responsabilidades:
//! - Detectar y destruir workers que deben ser terminados
//! - Manejar workers orphaned (existen en provider pero no en registry)
//! - Emitir eventos de cleanup
//! - Retry de destrucción fallida

use dashmap::DashMap;
use hodei_server_domain::{
    event_bus::EventBus,
    events::DomainEvent,
    outbox::{OutboxEventInsert, OutboxRepository},
    shared_kernel::{ProviderId, WorkerId, WorkerState},
    workers::{Worker, WorkerFilter, WorkerProvider, WorkerRegistry},
};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Resultado de la limpieza de workers
#[derive(Debug, Default)]
pub struct CleanupResult {
    /// Workers terminados exitosamente
    pub terminated: Vec<WorkerId>,
    /// Workers que fallaron en terminación
    pub failed: Vec<WorkerId>,
}

/// Resultado de cleanup de orphans
#[derive(Debug, Default)]
pub struct OrphanCleanupResult {
    /// Providers escaneados
    pub providers_scanned: usize,
    /// Orphans detectados
    pub orphans_detected: usize,
    /// Orphans limpiados
    pub orphans_cleaned: usize,
    /// Errores durante el proceso
    pub errors: usize,
    /// Duración en milisegundos
    pub duration_ms: u64,
}

/// Info de un worker orphaned (GAP-GO-02)
#[derive(Debug)]
pub(crate) struct OrphanWorkerInfo {
    pub worker_id: WorkerId,
    pub provider_resource_id: String,
    pub last_seen: DateTime<Utc>,
}

/// Configuración del WorkerGarbageCollector
#[derive(Debug, Clone)]
pub struct GarbageCollectorConfig {
    /// Intervalo entre limpiezas
    pub cleanup_interval: Duration,
    /// Máximo reintentos para destrucción
    pub max_destroy_retries: u32,
    /// Delay entre reintentos (exponential backoff)
    pub retry_base_delay: Duration,
}

impl Default for GarbageCollectorConfig {
    fn default() -> Self {
        Self {
            cleanup_interval: Duration::from_secs(30),
            max_destroy_retries: 3,
            retry_base_delay: Duration::from_secs(1),
        }
    }
}

/// WorkerGarbageCollector - Componente para limpieza de workers
///
/// Maneja la terminación de workers que ya no son necesarios y la
/// detección/limpieza de workers orphaned.
#[derive(Clone)]
pub struct WorkerGarbageCollector {
    /// Registry de workers
    registry: Arc<dyn WorkerRegistry>,
    /// Providers disponibles
    providers: Arc<DashMap<ProviderId, Arc<dyn WorkerProvider>>>,
    /// Event bus para publicación de eventos
    event_bus: Arc<dyn EventBus>,
    /// Outbox repository para publicación transaccional
    outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    /// Configuración
    config: GarbageCollectorConfig,
}

impl WorkerGarbageCollector {
    /// Crear nuevo WorkerGarbageCollector
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        providers: Arc<DashMap<ProviderId, Arc<dyn WorkerProvider>>>,
        event_bus: Arc<dyn EventBus>,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
        config: Option<GarbageCollectorConfig>,
    ) -> Self {
        Self {
            registry,
            providers,
            event_bus,
            outbox_repository,
            config: config.unwrap_or_default(),
        }
    }

    /// Ejecutar cleanup de todos los workers que deben ser terminados
    ///
    /// EPIC-21: Workers son efímeros - se terminan después del job
    /// EPIC-26 US-26.7: Usa políticas TTL del WorkerSpec
    pub async fn cleanup(&self) -> Result<CleanupResult, String> {
        let mut result = CleanupResult::default();

        // Encontrar candidatos para terminación
        let all_workers = self
            .registry
            .find(&WorkerFilter::new())
            .await
            .map_err(|e| format!("Failed to find workers: {}", e))?;

        let workers_to_terminate: Vec<_> = all_workers
            .iter()
            .filter(|w| {
                // Workers que deben terminarse:
                // 1. En estado Busy (ephemeral mode)
                // 2. En estado Terminated (retry de cleanup fallido)
                // 3. Listos y en idle timeout
                // 4. Lifetime excedido
                // 5. TTL after completion excedido
                matches!(*w.state(), WorkerState::Busy)
                    || matches!(*w.state(), WorkerState::Terminated)
                    || w.is_idle_timeout()
                    || w.is_lifetime_exceeded()
                    || w.is_ttl_after_completion_exceeded()
            })
            .collect();

        info!(
            "🧹 GC Cleanup: Terminating {} workers (using TTL policies)",
            workers_to_terminate.len()
        );

        // Emitir eventos de idle para workers en timeout
        for worker in &workers_to_terminate {
            if worker.is_idle_timeout() {
                self.emit_worker_idle_event(worker).await;
            }
        }

        // Terminar cada worker
        for worker in workers_to_terminate {
            let worker_id = worker.id().clone();
            let reason = worker.termination_reason();

            info!("Terminating worker {} (reason: {:?})", worker_id, reason);

            // Marcar como terminating si no lo está
            if !matches!(*worker.state(), WorkerState::Terminated) {
                if let Err(e) = self
                    .registry
                    .update_state(&worker_id, WorkerState::Terminated)
                    .await
                {
                    error!("Failed to mark worker {} as terminating: {}", worker_id, e);
                    continue;
                }
            }

            // Destruir via provider con retry
            match self.destroy_worker_with_retry(worker).await {
                Ok(_) => {
                    result.terminated.push(worker_id.clone());

                    // Desregistrar
                    if let Err(e) = self.registry.unregister(&worker_id).await {
                        warn!("Failed to unregister worker {}: {}", worker_id, e);
                    }
                }
                Err(e) => {
                    error!("Failed to destroy worker {}: {}", worker_id, e);
                    result.failed.push(worker_id);
                }
            }
        }

        Ok(result)
    }

    /// Destruir un worker con retry logic
    async fn destroy_worker_with_retry(&self, worker: &Worker) -> Result<(), String> {
        let provider = self
            .providers
            .get(worker.provider_id())
            .ok_or_else(|| format!("Provider not found: {}", worker.provider_id()))?;

        let max_retries = self.config.max_destroy_retries;
        let mut last_error = None;

        for attempt in 1..=max_retries {
            match provider.destroy_worker(worker.handle()).await {
                Ok(_) => {
                    info!(
                        "Worker {} destroyed successfully via provider {}",
                        worker.id(),
                        worker.provider_id()
                    );

                    // Emitir eventos de terminación
                    self.emit_termination_events(worker).await?;
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries {
                        let backoff = self.config.retry_base_delay * attempt;
                        warn!(
                            "Attempt {}/{} failed to destroy worker {}: Retrying in {}s",
                            attempt,
                            max_retries,
                            worker.id(),
                            backoff.as_secs()
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        Err(format!(
            "Failed to destroy worker {} after {} attempts: {:?}",
            worker.id(),
            max_retries,
            last_error
        ))
    }

    /// Emitir eventos de terminación
    async fn emit_termination_events(&self, worker: &Worker) -> Result<(), String> {
        let worker_id = worker.id().clone();
        let reason = worker.termination_reason();

        // WorkerEphemeralTerminating
        let event = DomainEvent::WorkerEphemeralTerminating {
            worker_id: worker_id.clone(),
            provider_id: worker.provider_id().clone(),
            reason: reason.clone(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: Some("garbage-collector".to_string()),
        };
        if let Err(e) = self.event_bus.publish(&event).await {
            warn!("Failed to publish WorkerEphemeralTerminating event: {}", e);
        }

        // WorkerTerminated para backwards compatibility
        let terminated_event = DomainEvent::WorkerTerminated {
            worker_id: worker_id.clone(),
            provider_id: worker.provider_id().clone(),
            reason,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: Some("garbage-collector".to_string()),
        };
        if let Err(e) = self.event_bus.publish(&terminated_event).await {
            warn!("Failed to publish WorkerTerminated event: {}", e);
        }

        Ok(())
    }

    /// Emitir evento WorkerEphemeralIdle
    async fn emit_worker_idle_event(&self, worker: &Worker) {
        let event = OutboxEventInsert::for_worker(
            worker.id().0,
            "WorkerEphemeralIdle".to_string(),
            serde_json::json!({
                "worker_id": worker.id().0.to_string(),
                "provider_id": worker.provider_id().0.to_string(),
                "idle_since": worker.last_heartbeat().to_rfc3339(),
                "idle_timeout_secs": worker.idle_timeout_secs(),
                "current_job_id": worker.current_job_id().map(|j| j.0.to_string())
            }),
            Some(serde_json::json!({
                "source": "WorkerGarbageCollector",
                "event": "idle_timeout_detected"
            })),
            Some(format!("worker-idle-{}", worker.id().0)),
        );

        if let Err(e) = self.outbox_repository.insert_events(&[event]).await {
            warn!("Failed to insert WorkerEphemeralIdle event: {:?}", e);
        }
    }

    /// Detectar y limpiar workers orphaned
    ///
    /// Orphan workers son workers que existen en el provider pero
    /// no están registrados en el registry.
    pub async fn detect_and_cleanup_orphans(&self) -> Result<OrphanCleanupResult, String> {
        let mut result = OrphanCleanupResult::default();
        let start_time = Utc::now();

        info!("🔍 Starting orphan worker detection...");

        let provider_ids: Vec<ProviderId> = self.providers.iter().map(|e| e.key().clone()).collect();

        for provider_id in provider_ids {
            let provider = match self.providers.get(&provider_id) {
                Some(p) => p.value().clone(),
                None => continue,
            };

            match self.detect_orphans_for_provider(&provider, provider_id.clone()).await {
                Ok(orphans) => {
                    result.providers_scanned += 1;
                    result.orphans_detected += orphans.len();

                    for orphan in orphans {
                        info!(
                            "🗑️ Destroying orphan worker {} from provider {}",
                            orphan.provider_resource_id, provider_id
                        );

                        // Emitir evento de detección
                        self.emit_orphan_detected(&orphan, &provider_id).await;

                        if let Err(e) = provider
                            .destroy_worker_by_id(&orphan.provider_resource_id)
                            .await
                        {
                            warn!(
                                "Failed to destroy orphan {} from provider {}: {}",
                                orphan.provider_resource_id, provider_id, e
                            );
                            result.errors += 1;
                        } else {
                            result.orphans_cleaned += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to detect orphans for provider {}: {}", provider_id, e);
                    result.errors += 1;
                }
            }
        }

        result.duration_ms = Utc::now()
            .signed_duration_since(start_time)
            .to_std()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if result.orphans_cleaned > 0 {
            info!(
                "🧹 Orphan cleanup complete: {} cleaned, {} detected, {} errors in {}ms",
                result.orphans_cleaned, result.orphans_detected, result.errors, result.duration_ms
            );
        }

        Ok(result)
    }

    /// Detectar orphans para un provider específico
    async fn detect_orphans_for_provider(
        &self,
        provider: &Arc<dyn WorkerProvider>,
        provider_id: ProviderId,
    ) -> Result<Vec<OrphanWorkerInfo>, String> {
        let mut orphans = Vec::new();

        // Workers del provider
        let provider_workers = provider
            .list_workers()
            .await
            .map_err(|e| format!("Failed to list workers from provider {}: {}", provider_id, e))?;

        // Workers registrados para este provider
        let registered_workers = self
            .registry
            .find_by_provider(&provider_id)
            .await
            .map_err(|e| format!("Failed to find registered workers for provider {}: {}", provider_id, e))?;
        let registered_ids: std::collections::HashSet<String> = registered_workers
            .iter()
            .map(|w| w.handle().provider_resource_id.clone())
            .collect();

        // Encontrar orphans
        for pw in provider_workers {
            if !registered_ids.contains(&pw.resource_id) {
                orphans.push(OrphanWorkerInfo {
                    worker_id: WorkerId::new(),
                    provider_resource_id: pw.resource_id,
                    last_seen: pw.last_seen.unwrap_or_else(Utc::now),
                });
            }
        }

        Ok(orphans)
    }

    /// Emitir evento OrphanWorkerDetected
    async fn emit_orphan_detected(&self, orphan: &OrphanWorkerInfo, provider_id: &ProviderId) {
        let now = Utc::now();
        let orphaned_duration = now
            .signed_duration_since(orphan.last_seen)
            .to_std()
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let event = OutboxEventInsert::for_worker(
            orphan.worker_id.0,
            "OrphanWorkerDetected".to_string(),
            serde_json::json!({
                "worker_id": orphan.worker_id.0.to_string(),
                "provider_id": provider_id.0.to_string(),
                "last_seen": orphan.last_seen.to_rfc3339(),
                "orphaned_duration_secs": orphaned_duration,
                "detection_method": "garbage_collection"
            }),
            Some(serde_json::json!({
                "source": "WorkerGarbageCollector",
                "provider_resource_id": orphan.provider_resource_id
            })),
            Some(format!(
                "orphan-detected-{}-{}",
                provider_id.0,
                now.timestamp()
            )),
        );

        if let Err(e) = self.outbox_repository.insert_events(&[event]).await {
            warn!("Failed to insert OrphanWorkerDetected event: {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::workers::WorkerHandle;
    use std::time::Duration;

    #[tokio::test]
    async fn test_cleanup_result_default() {
        let result = CleanupResult::default();
        assert!(result.terminated.is_empty());
        assert!(result.failed.is_empty());
    }

    #[tokio::test]
    async fn test_orphan_cleanup_result_default() {
        let result = OrphanCleanupResult::default();
        assert_eq!(result.providers_scanned, 0);
        assert_eq!(result.orphans_detected, 0);
        assert_eq!(result.orphans_cleaned, 0);
        assert_eq!(result.errors, 0);
    }

    #[tokio::test]
    async fn test_config_default() {
        let config = GarbageCollectorConfig::default();
        assert_eq!(config.cleanup_interval, Duration::from_secs(30));
        assert_eq!(config.max_destroy_retries, 3);
        assert_eq!(config.retry_base_delay, Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_cleanup_result_with_data() {
        let mut result = CleanupResult::default();
        let worker1 = WorkerId::new();
        let worker2 = WorkerId::new();

        result.terminated.push(worker1.clone());
        result.failed.push(worker2.clone());

        assert_eq!(result.terminated.len(), 1);
        assert_eq!(result.failed.len(), 1);
    }
}
