//! Worker Monitor Component
//!
//! Responsible for monitoring worker health and connectivity status.
//! Follows Single Responsibility Principle: only handles worker monitoring.

use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::shared_kernel::{Result, WorkerId, WorkerState};
use hodei_server_domain::workers::WorkerRegistry;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, warn};

/// Worker Monitor
///
/// Monitors worker health and connectivity, responsible for:
/// - Tracking worker heartbeats
/// - Detecting disconnected workers
/// - Publishing worker status events
/// - Cleaning up stale worker records
pub struct WorkerMonitor {
    worker_registry: Arc<dyn WorkerRegistry>,
    event_bus: Arc<dyn EventBus>,
    heartbeat_timeout: Duration,
    cleanup_interval: Duration,
}

impl WorkerMonitor {
    /// Create a new WorkerMonitor
    pub fn new(worker_registry: Arc<dyn WorkerRegistry>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            worker_registry,
            event_bus,
            heartbeat_timeout: Duration::from_secs(30),
            cleanup_interval: Duration::from_secs(60),
        }
    }

    /// Set custom heartbeat timeout
    /// Builder Pattern: returns self for fluent configuration
    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = timeout;
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
    pub async fn start(&self) -> Result<tokio::sync::mpsc::Receiver<()>> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        let worker_registry = self.worker_registry.clone();
        let event_bus = self.event_bus.clone();
        let heartbeat_timeout = self.heartbeat_timeout;
        let cleanup_interval = self.cleanup_interval;
        let shutdown_tx = shutdown_tx.clone();

        tokio::spawn(async move {
            info!("👁️ WorkerMonitor: Starting monitoring loop");
            let mut interval = time::interval(cleanup_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = monitor_workers(&worker_registry, &event_bus, heartbeat_timeout).await {
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
    pub async fn stop(&self, _shutdown_rx: tokio::sync::mpsc::Receiver<()>) -> Result<()> {
        // The shutdown receiver will be dropped, causing the monitoring loop to exit
        Ok(())
    }

    /// Check if a specific worker is healthy
    pub async fn is_worker_healthy(&self, worker_id: &WorkerId) -> Result<bool> {
        let worker = self.worker_registry.get(worker_id).await?;

        match worker {
            Some(w) => {
                let now = Utc::now();
                let heartbeat_age = now
                    .signed_duration_since(w.last_heartbeat())
                    .to_std()
                    .unwrap_or(Duration::MAX);

                let is_healthy = heartbeat_age < self.heartbeat_timeout;
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
    pub async fn get_healthy_workers(&self) -> Result<Vec<WorkerId>> {
        let workers = self.worker_registry.find_available().await?;
        let now = Utc::now();

        let healthy_workers: Vec<WorkerId> = workers
            .into_iter()
            .filter(|worker| {
                let heartbeat_age = now
                    .signed_duration_since(worker.last_heartbeat())
                    .to_std()
                    .unwrap_or(Duration::MAX);

                heartbeat_age < self.heartbeat_timeout
            })
            .map(|worker| worker.id().clone())
            .collect();

        debug!(
            "WorkerMonitor: Found {} healthy workers",
            healthy_workers.len()
        );
        Ok(healthy_workers)
    }
}

/// Monitor workers for health and connectivity
async fn monitor_workers(
    worker_registry: &Arc<dyn WorkerRegistry>,
    event_bus: &Arc<dyn EventBus>,
    heartbeat_timeout: Duration,
) -> Result<()> {
    let workers = worker_registry.find_available().await?;
    let now = Utc::now();
    let mut disconnected_workers = Vec::new();

    for worker in workers {
        let heartbeat_age = now
            .signed_duration_since(worker.last_heartbeat())
            .to_std()
            .unwrap_or(Duration::MAX);

        if heartbeat_age >= heartbeat_timeout {
            debug!(
                "WorkerMonitor: Worker {} is disconnected (heartbeat age: {:?})",
                worker.id(),
                heartbeat_age
            );
            disconnected_workers.push(worker.id().clone());
        }
    }

    // Publish WorkerDisconnected events for stale workers
    for worker_id in disconnected_workers {
        let event = DomainEvent::WorkerStatusChanged {
            worker_id: worker_id.clone(),
            old_status: WorkerState::Ready,
            new_status: WorkerState::Terminated,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        if let Err(e) = event_bus.publish(&event).await {
            error!(
                "WorkerMonitor: Failed to publish WorkerStatusChanged event: {}",
                e
            );
        } else {
            info!(
                "📢 WorkerMonitor: Published WorkerDisconnected event for worker {}",
                worker_id
            );
        }
    }

    Ok(())
}
