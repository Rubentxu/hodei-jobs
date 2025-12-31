//! Job Coordination Component
//!
//! Orchestrates the job processing workflow by coordinating:
//! - EventSubscriber: receives events and triggers processing
//! - JobDispatcher: processes jobs and dispatches to workers
//! - WorkerMonitor: monitors worker health
//!
//! This is a thin orchestration layer that delegates to specialized components.
//!
//! ## EPIC-29: Reactive Event-Driven Architecture
//! This component has been refactored to use event-driven reactivity instead of polling.
//! - Subscribe to JobQueued events to trigger job processing
//! - Subscribe to WorkerReadyForJob events to trigger dispatch
//! - Eliminated continuous polling loop

use crate::jobs::dispatcher::JobDispatcher;
use crate::jobs::worker_monitor::WorkerMonitor;
use futures::StreamExt;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Job Coordinator
///
/// Orchestrates the job processing workflow by coordinating:
/// - Event subscription and routing
/// - Job dispatching
/// - Worker monitoring
///
/// This component follows the Facade pattern, providing a simple interface
/// to the complex subsystem of job processing.
///
/// ## EPIC-29: Reactive Mode
/// Uses event-driven reactivity instead of polling:
/// - Listens to JobQueued → triggers provisioning or dispatch
/// - Listens to WorkerReadyForJob → triggers job dispatch
pub struct JobCoordinator {
    event_bus: Arc<dyn EventBus>,
    job_dispatcher: Arc<JobDispatcher>,
    worker_monitor: Arc<WorkerMonitor>,
    monitor_shutdown: Option<mpsc::Receiver<()>>,
}

impl JobCoordinator {
    /// Create a new JobCoordinator
    pub fn new(
        event_bus: Arc<dyn EventBus>,
        job_dispatcher: Arc<JobDispatcher>,
        worker_monitor: Arc<WorkerMonitor>,
    ) -> Self {
        Self {
            event_bus,
            job_dispatcher,
            worker_monitor,
            monitor_shutdown: None,
        }
    }

    /// Start the coordinator in reactive mode (EPIC-29)
    ///
    /// This will start:
    /// 1. Worker monitoring (to track worker health)
    /// 2. Event subscription for JobQueued and WorkerReadyForJob
    /// 3. Reactive job processing (no polling loop)
    ///
    /// Returns: Result<()>
    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!("🚀 JobCoordinator: Starting job processing system (EPIC-29 Reactive Mode)");

        // Start worker monitor and keep the shutdown signal alive
        let monitor_shutdown = self.worker_monitor.start().await?;
        self.monitor_shutdown = Some(monitor_shutdown);
        info!("👁️ JobCoordinator: Worker monitor started");

        // EPIC-29: Start reactive event processing instead of polling loop
        self.start_reactive_event_processing().await?;

        info!("✅ JobCoordinator: Job processing system started successfully");

        Ok(())
    }

    /// EPIC-29: Start reactive event processing
    ///
    /// Subscribes to domain events and triggers processing reactively:
    /// - JobQueued → handle_job_queued()
    /// - WorkerReadyForJob → dispatch_pending_job()
    async fn start_reactive_event_processing(&mut self) -> anyhow::Result<()> {
        let event_bus = self.event_bus.clone();
        let job_dispatcher = self.job_dispatcher.clone();

        // Subscribe to JobQueued events (EPIC-31: Use hodei_events channel for all domain events)
        let mut job_queue_stream = event_bus
            .subscribe("hodei_events")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to hodei_events: {}", e))?;

        // Subscribe to WorkerReadyForJob events (EPIC-31: Use hodei_events channel)
        let mut worker_ready_stream = event_bus
            .subscribe("hodei_events")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to hodei_events: {}", e))?;

        // Get a reference to self for handling events
        let dispatcher = self.job_dispatcher.clone();

        // Spawn event processing task
        tokio::spawn(async move {
            info!("🔄 JobCoordinator: Starting reactive event processing");

            loop {
                tokio::select! {
                    // Process JobQueued events
                    event_result = job_queue_stream.next() => {
                        match event_result {
                            Some(Ok(event)) => {
                                if let DomainEvent::JobQueued { job_id, .. } = event {
                                    info!("📦 JobCoordinator: Received JobQueued event for job {}", job_id);
                                    dispatcher.handle_job_queued(&job_id).await;
                                }
                            }
                            Some(Err(e)) => {
                                error!("❌ JobCoordinator: Error receiving JobQueued event: {}", e);
                            }
                            None => {
                                warn!("⚠️ JobCoordinator: JobQueued stream ended, reconnecting...");
                                // In production, implement reconnection logic
                                tokio::time::sleep(Duration::from_secs(5)).await;
                            }
                        }
                    }

                    // Process WorkerReadyForJob events
                    event_result = worker_ready_stream.next() => {
                        match event_result {
                            Some(Ok(event)) => {
                                if let DomainEvent::WorkerReadyForJob { worker_id, .. } = event {
                                    info!("👷 JobCoordinator: Received WorkerReadyForJob event for worker {}", worker_id);
                                    dispatcher.dispatch_pending_job_to_worker(&worker_id).await;
                                }
                            }
                            Some(Err(e)) => {
                                error!("❌ JobCoordinator: Error receiving WorkerReadyForJob event: {}", e);
                            }
                            None => {
                                warn!("⚠️ JobCoordinator: WorkerReadyForJob stream ended, reconnecting...");
                                tokio::time::sleep(Duration::from_secs(5)).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start the coordinator in legacy polling mode (deprecated, for migration only)
    ///
    /// This method is kept for backwards compatibility and gradual migration.
    /// Use `start()` for new deployments (EPIC-29 reactive mode).
    pub async fn start_polling_mode(&mut self) -> anyhow::Result<()> {
        info!("🚀 JobCoordinator: Starting job processing system (Legacy Polling Mode)");

        // Start worker monitor
        let monitor_shutdown = self.worker_monitor.start().await?;
        self.monitor_shutdown = Some(monitor_shutdown);

        // Start legacy polling loop
        let job_dispatcher = self.job_dispatcher.clone();
        tokio::spawn(async move {
            info!("🔄 JobCoordinator: Starting legacy polling loop (DEPRECATED)");
            loop {
                match job_dispatcher.dispatch_once().await {
                    Ok(0) => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    Ok(count) => {
                        debug!("✅ JobCoordinator: Processed {} jobs", count);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(e) => {
                        error!("❌ JobCoordinator: Error during job dispatch: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(())
    }

    /// Manually trigger a job dispatch cycle
    /// This is useful for testing or manual triggering
    pub async fn dispatch_now(&self) -> anyhow::Result<usize> {
        info!("🔄 JobCoordinator: Manual dispatch trigger");
        self.job_dispatcher.dispatch_once().await
    }
}
