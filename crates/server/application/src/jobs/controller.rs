//! Job Controller - Refactored to use specialized components
//!
//! This is now a thin facade that delegates to specialized components:
//! - EventSubscriber: handles event subscription
//! - JobDispatcher: handles job dispatching
//! - WorkerMonitor: handles worker monitoring
//! - JobCoordinator: orchestrates the workflow
//!
//! This refactoring follows the Single Responsibility Principle by separating
//! concerns into focused components instead of having a "God Object".

use crate::jobs::coordinator::JobCoordinator;
use crate::providers::ProviderRegistry;
use crate::scheduling::smart_scheduler::SchedulerConfig;
use crate::workers::commands::WorkerCommandSender;
use crate::workers::provisioning::WorkerProvisioningService;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::jobs::{JobQueue, JobRepository};
use hodei_server_domain::shared_kernel::Result;
use hodei_server_domain::workers::WorkerRegistry;
use std::sync::Arc;
use tracing::info;

/// Job Controller (Refactored - Facade Pattern)
///
/// This is now a thin facade that delegates to specialized components.
/// The actual work is done by:
/// - JobCoordinator: orchestrates the workflow
/// - EventSubscriber: subscribes to events
/// - JobDispatcher: dispatches jobs
/// - WorkerMonitor: monitors workers
///
/// This follows the Single Responsibility Principle and eliminates the "God Object" anti-pattern.
pub struct JobController {
    coordinator: JobCoordinator,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
}

impl JobController {
    /// Create a new JobController with all dependencies
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        provider_registry: Arc<ProviderRegistry>,
        scheduler_config: SchedulerConfig,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
        event_bus: Arc<dyn EventBus>,
        provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
    ) -> Self {
        info!("Initializing JobController with specialized components");

        // Create specialized components
        let job_dispatcher = Arc::new(crate::jobs::dispatcher::JobDispatcher::new(
            job_queue,
            job_repository,
            worker_registry.clone(),
            provider_registry.clone(),
            scheduler_config,
            worker_command_sender,
            event_bus.clone(),
            provisioning_service.clone(),
        ));

        let worker_monitor = Arc::new(crate::jobs::worker_monitor::WorkerMonitor::new(
            worker_registry,
            event_bus.clone(),
        ));

        // Create coordinator that orchestrates everything
        let coordinator = JobCoordinator::new(event_bus, job_dispatcher, worker_monitor);

        Self {
            coordinator,
            provisioning_service,
        }
    }

    /// Start the job controller
    /// This will start:
    /// - Event subscription
    /// - Worker monitoring
    /// - Job dispatching coordination
    ///
    /// Returns: Result<()>
    pub async fn start(&mut self) -> Result<()> {
        self.coordinator.start().await
    }

    /// Manually trigger a job dispatch cycle
    /// This is useful for testing or manual triggering
    pub async fn dispatch_now(&self) -> Result<usize> {
        self.coordinator.dispatch_now().await
    }

    /// Legacy method for backward compatibility
    /// Delegates to the coordinator
    pub async fn run_once(&self) -> Result<usize> {
        self.dispatch_now().await
    }
}
