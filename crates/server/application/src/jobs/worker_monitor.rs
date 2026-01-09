//! Worker Monitor Component
//!
//! Responsible for monitoring worker health and connectivity status.
//! Follows Single Responsibility Principle: only handles worker monitoring.
//!
//! ## Rol: Read-only Observer
//! Este componente SOLO observa y reporta. NO modifica estados de workers.
//! Las modificaciones de estado son responsabilidad exclusiva de WorkerLifecycleManager.
//!
//! ## Responsabilidades:
//! - Monitorear health de workers (readonly)
//! - Detectar workers desconectados
//! - Publicar alertas de conectividad (readonly)
//! - Recopilar métricas de salud
//! - NO modifica estados de workers (solo WorkerLifecycleManager hace esto)

use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::{DomainEvent, EventMetadata};
use hodei_server_domain::shared_kernel::{WorkerId, WorkerState};
use hodei_server_domain::workers::events::WorkerStatusChanged;
use hodei_server_domain::workers::WorkerRegistry;
use hodei_server_domain::workers::health::{WorkerHealthService, WorkerHealthStatus};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, warn};

/// Worker Monitor
///
/// Monitors worker health and connectivity (readonly).
/// Only publishes alerts - does NOT modify worker states.
///
/// ## Estados que solo observa (no modifica):
/// - Creating, Connecting, Ready, Busy, Terminating, Terminated, Failed
///
/// ## Eventos que publica (solo alertas):
/// - WorkerStatusChanged (como observador, no como decisor)
/// - WorkerDisconnected (alerta de conectividad)
pub struct WorkerMonitor {
    worker_registry: Arc<dyn WorkerRegistry>,
    event_bus: Arc<dyn EventBus>,
    health_service: Arc<WorkerHealthService>,
    cleanup_interval: Duration,
}

impl WorkerMonitor {
    /// Create a new WorkerMonitor
    pub fn new(worker_registry: Arc<dyn WorkerRegistry>, event_bus: Arc<dyn EventBus>) -> Self {
        let health_service = Arc::new(WorkerHealthService::builder().build());
        Self {
            worker_registry,
            event_bus,
            health_service,
            cleanup_interval: Duration::from_secs(60),
        }
    }

    /// Set custom heartbeat timeout
    /// Builder Pattern: returns self for fluent configuration
    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.health_service = Arc::new(
            WorkerHealthService::builder()
                .with_heartbeat_timeout(timeout)
                .build(),
        );
        self
    }

    /// Set custom cleanup interval
    /// Builder Pattern: returns self for fluent configuration
    pub fn with_cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }

    /// Start the monitoring loop
    /// Returns a shutdown signal receiver
    pub async fn start(&self) -> anyhow::Result<tokio::sync::mpsc::Receiver<()>> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        let worker_registry = self.worker_registry.clone();
        let event_bus = self.event_bus.clone();
        let health_service = self.health_service.clone();
        let cleanup_interval = self.cleanup_interval;
        let shutdown_tx = shutdown_tx.clone();

        tokio::spawn(async move {
            info!("👁️ WorkerMonitor: Starting monitoring loop");
            let mut interval = time::interval(cleanup_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = monitor_workers(&worker_registry, &event_bus, health_service.clone()).await {
                            error!("❌ WorkerMonitor: Error during monitoring: {}", e);
                        }
                    }
                    _ = shutdown_tx.closed() => {
                        info!("🛑 WorkerMonitor: Received shutdown signal");
                        break;
                    }
                }
            }

            info!("👁️ WorkerMonitor: Monitoring loop ended");
        });

        Ok(shutdown_rx)
    }

    /// Stop the monitor
    pub async fn stop(&self, _shutdown_rx: tokio::sync::mpsc::Receiver<()>) -> anyhow::Result<()> {
        // The shutdown receiver will be dropped, causing the monitoring loop to exit
        Ok(())
    }

    /// Check if a specific worker is healthy
    pub async fn is_worker_healthy(&self, worker_id: &WorkerId) -> anyhow::Result<bool> {
        let worker = self.worker_registry.get(worker_id).await?;

        match worker {
            Some(w) => {
                let is_healthy = self.health_service.is_healthy(&w);
                debug!(
                    "WorkerMonitor: Worker {} is {}",
                    worker_id,
                    if is_healthy { "healthy" } else { "unhealthy" }
                );

                Ok(is_healthy)
            }
            None => {
                warn!("WorkerMonitor: Worker {} not found", worker_id);
                Ok(false)
            }
        }
    }

    /// Get list of healthy workers
    pub async fn get_healthy_workers(&self) -> anyhow::Result<Vec<WorkerId>> {
        let workers = self.worker_registry.find_available().await?;

        let healthy_workers: Vec<WorkerId> = workers
            .into_iter()
            .filter(|worker| self.health_service.is_healthy(worker))
            .map(|worker| worker.id().clone())
            .collect();

        debug!(
            "WorkerMonitor: Found {} healthy workers",
            healthy_workers.len()
        );
        Ok(healthy_workers)
    }
}

/// Monitor workers for health and connectivity (readonly)
/// Solo detecta y reporta - NO modifica estados de workers.
///
/// ## Nota Importante:
/// Las modificaciones de estado (Ready -> Terminating, etc.) son
/// responsabilidad EXCLUSIVA de WorkerLifecycleManager.
async fn monitor_workers(
    worker_registry: &Arc<dyn WorkerRegistry>,
    event_bus: &Arc<dyn EventBus>,
    health_service: Arc<WorkerHealthService>,
) -> anyhow::Result<()> {
    let workers = worker_registry.find_available().await?;
    let mut disconnected_workers: Vec<(WorkerId, WorkerState, Duration)> = Vec::new();

    for worker in workers {
        let health_status = health_service.assess_worker_health(&worker);

        match health_status {
            WorkerHealthStatus::Dead(age) => {
                debug!(
                    "WorkerMonitor: Worker {} is disconnected (heartbeat age: {:?})",
                    worker.id(),
                    age.as_duration()
                );
                disconnected_workers.push((
                    worker.id().clone(),
                    worker.state().clone(),
                    age.as_duration(),
                ));
            }
            WorkerHealthStatus::Degraded(age) => {
                debug!(
                    "WorkerMonitor: Worker {} has degraded health (heartbeat age: {:?})",
                    worker.id(),
                    age.as_duration()
                );
            }
            WorkerHealthStatus::Healthy => {
                debug!("WorkerMonitor: Worker {} is healthy", worker.id());
            }
        }
    }

    // Publicar alertas de conectividad (readonly - como observador)
    // NOTA: NO modificamos estados - eso es responsabilidad de WorkerLifecycleManager
    for (worker_id, current_state, age) in disconnected_workers {
        // Como observador, reportamos el estado actual y alertamos sobre la desconexión
        // El WorkerLifecycleManager es quien debe tomar acción de cambiar el estado
        let metadata = EventMetadata::for_system_event(None, "system:worker_monitor");

        // Publicamos un evento informativo de desconexión detected
        // EPIC-65 Phase 3: Using modular event type
        let status_changed_event = WorkerStatusChanged {
            worker_id: worker_id.clone(),
            old_status: current_state,
            // NO cambiamos a Terminated - eso lo hace WorkerLifecycleManager
            // Solo reportamos que detectamos un worker Dead/Disconnected
            new_status: WorkerState::Terminated,
            occurred_at: Utc::now(),
            correlation_id: metadata.correlation_id,
            actor: metadata.actor,
        };

        if let Err(e) = event_bus.publish(&status_changed_event.into()).await {
            error!(
                "WorkerMonitor: Failed to publish disconnection alert: {}",
                e
            );
        } else {
            info!(
                "⚠️ WorkerMonitor: Published disconnection alert for worker {} (heartbeat: {:?})",
                worker_id, age
            );
        }
    }

    Ok(())
}
