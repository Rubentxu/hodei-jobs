//! Job Coordination Component
//!
//! Orchestrates the job processing workflow by coordinating:
//! - EventSubscriber: receives events and triggers processing
//! - JobDispatcher: processes jobs and dispatches to workers
//! - WorkerMonitor: monitors worker health
//!
//! This is a thin orchestration layer that delegates to specialized components.

use crate::jobs::dispatcher::JobDispatcher;
use crate::jobs::worker_monitor::WorkerMonitor;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::shared_kernel::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Job Coordinator
///
/// Orchestrates the job processing workflow by coordinating:
/// - Event subscription and routing
/// - Job dispatching
/// - Worker monitoring
///
/// This component follows the Facade pattern, providing a simple interface
/// to the complex subsystem of job processing.
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

    /// Start the coordinator
    /// This will start:
    /// 1. Event subscription (to trigger processing on events)
    /// 2. Worker monitoring (to track worker health)
    /// 3. Continuous job processing loop
    ///
    /// Returns: Result<()>
    pub async fn start(&mut self) -> Result<()> {
        info!("🚀 JobCoordinator: Starting job processing system");

        // Start worker monitor and keep the shutdown signal alive
        let monitor_shutdown = self.worker_monitor.start().await?;
        self.monitor_shutdown = Some(monitor_shutdown);
        info!("👁️ JobCoordinator: Worker monitor started");

        // Start continuous job processing loop
        let job_dispatcher = self.job_dispatcher.clone();
        tokio::spawn(async move {
            info!("🔄 JobCoordinator: Starting continuous job processing loop");
            loop {
                // Process jobs
                match job_dispatcher.dispatch_once().await {
                    Ok(0) => {
                        // No jobs processed, wait a bit before checking again
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    Ok(count) => {
                        debug!("✅ JobCoordinator: Processed {} jobs", count);
                        // Small delay between batches
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(e) => {
                        error!("❌ JobCoordinator: Error during job dispatch: {}", e);
                        // Wait a bit before retrying on error
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        info!("✅ JobCoordinator: Job processing system started successfully");

        Ok(())
    }

    /// Manually trigger a job dispatch cycle
    /// This is useful for testing or manual triggering
    pub async fn dispatch_now(&self) -> Result<usize> {
        info!("🔄 JobCoordinator: Manual dispatch trigger");
        self.job_dispatcher.dispatch_once().await
    }
}
