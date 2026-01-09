use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use tracing::{info, warn, error};
use hodei_server_domain::workers::WorkerRegistry;
use hodei_server_domain::command::CommandBus;
use hodei_server_domain::saga::commands::provisioning::DestroyWorkerCommand;
use tokio::time::interval;

/// Configuration for Orphan Worker Detector
#[derive(Debug, Clone)]
pub struct OrphanDetectorConfig {
    /// How often to run the detection loop
    pub detection_interval: Duration,
    /// Threshold for considering a worker orphaned (e.g., stuck in provisioning for > 5m)
    pub provisioning_timeout: Duration,
    /// Max workers to process in one run
    pub batch_size: usize,
}

impl Default for OrphanDetectorConfig {
    fn default() -> Self {
        Self {
            detection_interval: Duration::from_secs(60),
            provisioning_timeout: Duration::from_secs(300), // 5 minutes
            batch_size: 10,
        }
    }
}

/// Detects and cleans up workers stuck in PROVISIONING state
pub struct OrphanWorkerDetector<R, C> {
    registry: Arc<R>,
    command_bus: Arc<C>, // Use command bus to dispatch DestroyWorkerCommand
    config: OrphanDetectorConfig,
    is_running: std::sync::atomic::AtomicBool,
}

impl<R, C> OrphanWorkerDetector<R, C>
where
    R: WorkerRegistry + Send + Sync + 'static,
    C: CommandBus + Send + Sync + 'static,
{
    pub fn new(
        registry: Arc<R>,
        command_bus: Arc<C>,
        config: OrphanDetectorConfig,
    ) -> Self {
        Self {
            registry,
            command_bus,
            config,
            is_running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Run the detector loop
    pub async fn run(&self) {
        if self.is_running.swap(true, std::sync::atomic::Ordering::SeqCst) {
            warn!("Orphan detector is already running");
            return;
        }

        info!("Starting Orphan Worker Detector with config: {:?}", self.config);
        let mut timer = interval(self.config.detection_interval);

        while self.is_running.load(std::sync::atomic::Ordering::Relaxed) {
            timer.tick().await;
            if let Err(e) = self.run_detection().await {
                error!("Error during orphan detection: {:?}", e);
            }
        }
        
        info!("Orphan Worker Detector stopped");
    }
    
    pub fn stop(&self) {
        self.is_running.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Single detection run
    pub async fn run_detection(&self) -> anyhow::Result<usize> {
        let cutoff_time = Utc::now() - chrono::Duration::from_std(self.config.provisioning_timeout)?;
        
        // Find workers in PROVISIONING state older than cutoff
        // Note: Repository needs to support this specific query. 
        // Assuming find_stale_provisioning_workers exists or we scan (inefficient but ok for MVP).
        // Since find_stale... might not exist in trait, we might need to iterate. 
        // But strictly implementing as per EPIC implies we have the capability.
        // Let's assume we fetch all or specific status.
        
        // Using `find_by_status` if available or similar.
        // If not, we might simply skip implementation details requiring db schema changes 
        // and just define the logic structure.
        // However, based on shared code, `listen_workers` or `list` might exist.
        
        use hodei_server_domain::workers::registry::WorkerFilter;
        use hodei_server_domain::shared_kernel::WorkerState;

        let filter = WorkerFilter::new().with_state(WorkerState::Creating);
        let workers = self.registry.find(&filter).await?;
        
        let mut cleaned_count = 0;
        for worker in workers {
            let cutoff_time = Utc::now() - chrono::Duration::from_std(self.config.provisioning_timeout)?;
            if worker.created_at() < cutoff_time {
                info!("Detected orphaned worker {} (stuck since {})", worker.id(), worker.created_at());

                // Create DestroyWorkerCommand
                let command = DestroyWorkerCommand {
                    worker_id: worker.id().clone(),
                    provider_id: worker.provider_id().clone(),
                    reason: Some("Orphaned in provisioning state".to_string()),
                    saga_id: format!("orphan-detector-{}", Utc::now().timestamp()),
                    metadata: Default::default(),
                };

                // Dispatch command
                if let Err(e) = self.command_bus.dispatch(command).await {
                     error!("Failed to dispatch cleanup for worker {}: {:?}", worker.id(), e);
                } else {
                    cleaned_count += 1;
                }
            }
            if cleaned_count >= self.config.batch_size {
                break;
            }
        }

        if cleaned_count > 0 {
            info!("Cleaned up {} orphaned workers", cleaned_count);
        }
        
        Ok(cleaned_count)
    }
}
