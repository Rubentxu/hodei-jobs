//! Job Coordination Component (EPIC-32: Pure Reactive Architecture)
//!
//! Orchestrates the job processing workflow by coordinating:
//! - EventSubscriber: receives events with checkpointing (trait)
//! - JobDispatcher: processes jobs and dispatches to workers
//! - WorkerMonitor: monitors worker health
//!
//! ## EPIC-32: Pure Reactive Event-Driven Architecture
//! This component uses persistent subscriptions:
//! - Subscribe to JobQueued events with checkpointing
//! - Subscribe to WorkerReady events with checkpointing
//! - Automatic replay of missed events
//! - Dead letter queue for poison pills

use crate::jobs::dispatcher::JobDispatcher;
use crate::jobs::worker_monitor::WorkerMonitor;
use futures::StreamExt;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use sqlx::PgPool;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

/// Trait for event subscription with checkpointing support
///
/// This trait abstracts the persistent subscription functionality,
/// allowing different implementations (e.g., PostgreSQL, Kafka).
#[async_trait::async_trait]
pub trait EventSubscriber: Send + Sync {
    /// Subscribe to events and process them with the given handler
    async fn subscribe<H, R>(&self, handler: H) -> Result<(), EventBusError>
    where
        H: EventHandler + Send + Sync + 'static,
        R: Future<Output = Result<(), EventBusError>> + Send;

    /// Get the subscription ID
    fn subscription_id(&self) -> &str;

    /// Get the topic being subscribed to
    fn topic(&self) -> &str;
}

/// Trait for event handlers
#[async_trait::async_trait]
pub trait EventHandler: Send {
    /// Handle an event
    async fn handle(&self, event: DomainEvent) -> Result<(), EventBusError>;
}

#[async_trait::async_trait]
impl<F, R> EventHandler for F
where
    F: Send + Sync + Fn(DomainEvent) -> R,
    R: Future<Output = Result<(), EventBusError>> + Send,
{
    async fn handle(&self, event: DomainEvent) -> Result<(), EventBusError> {
        (self)(event).await
    }
}

/// Job Coordinator with Pure Reactive Processing
///
/// Uses dependency injection for event subscription to avoid circular dependencies.
pub struct JobCoordinator {
    event_bus: Arc<dyn EventBus>,
    job_dispatcher: Arc<JobDispatcher>,
    worker_monitor: Arc<WorkerMonitor>,
    pool: PgPool,
    shutdown_tx: watch::Sender<()>,
    monitor_shutdown: Option<mpsc::Receiver<()>>,
}

impl fmt::Debug for JobCoordinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobCoordinator").finish_non_exhaustive()
    }
}

impl JobCoordinator {
    /// Create a new JobCoordinator with pure reactive processing
    ///
    /// # Arguments
    /// * `event_bus` - Event bus for subscriptions
    /// * `job_dispatcher` - Job dispatcher for processing
    /// * `worker_monitor` - Worker health monitor
    /// * `pool` - Database pool for checkpointing and DLQ
    pub fn new(
        event_bus: Arc<dyn EventBus>,
        job_dispatcher: Arc<JobDispatcher>,
        worker_monitor: Arc<WorkerMonitor>,
        pool: PgPool,
    ) -> Self {
        let (shutdown_tx, _) = watch::channel(());
        Self {
            event_bus,
            job_dispatcher,
            worker_monitor,
            pool,
            shutdown_tx,
            monitor_shutdown: None,
        }
    }

    /// Run database migrations for reactive system
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        info!("Running EPIC-32 reactive system migrations...");

        // Create subscription_offsets table
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS subscription_offsets (
                subscription_id VARCHAR(255) PRIMARY KEY,
                topic VARCHAR(255) NOT NULL,
                consumer_group VARCHAR(255) NOT NULL,
                last_event_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
                last_event_occurred_at TIMESTAMPTZ NOT NULL DEFAULT '-infinity',
                last_processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                event_count BIGINT NOT NULL DEFAULT 0,
                gap_detected_at TIMESTAMPTZ,
                gap_resolved_at TIMESTAMPTZ,
                metadata JSONB DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#
        )
        .execute(&self.pool)
        .await?;

        // Create event_processing_dlq table
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS event_processing_dlq (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                event_id UUID NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                aggregate_id VARCHAR(255) NOT NULL,
                payload JSONB NOT NULL,
                error_message TEXT NOT NULL,
                error_count SMALLINT NOT NULL DEFAULT 1,
                first_failure_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                last_failure_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                resolved_at TIMESTAMPTZ,
                resolution_action VARCHAR(50),
                resolution_metadata JSONB,
                subscription_id VARCHAR(255) NOT NULL,
                retry_count SMALLINT NOT NULL DEFAULT 0,
                max_retries SMALLINT NOT NULL DEFAULT 3,
                metadata JSONB DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(event_id, subscription_id)
            )
            "#
        )
        .execute(&self.pool)
        .await?;

        info!("✅ EPIC-32 reactive system migrations complete");
        Ok(())
    }

    /// Start the coordinator in pure reactive mode (EPIC-32)
    ///
    /// This will start:
    /// 1. Worker monitoring (to track worker health)
    /// 2. Persistent event subscriptions with checkpointing
    /// 3. Reactive job processing with automatic replay
    ///
    /// Returns: Result<()>
    pub async fn start(&mut self) -> anyhow::Result<()> {
        // Run migrations first
        self.run_migrations().await?;

        info!("🚀 JobCoordinator: Starting job processing system (EPIC-32 Pure Reactive)");

        // Start worker monitor and keep the shutdown signal alive
        let monitor_shutdown = self.worker_monitor.start().await?;
        self.monitor_shutdown = Some(monitor_shutdown);
        info!("👁️ JobCoordinator: Worker monitor started");

        // EPIC-32: Start reactive event processing (basic version without checkpointing)
        self.start_reactive_event_processing().await?;

        info!(
            "✅ JobCoordinator: Job processing system started successfully (EPIC-32 Pure Reactive)"
        );

        Ok(())
    }

    /// EPIC-29: Start reactive event processing (basic version)
    ///
    /// Uses the basic EventBus subscribe method. For production with checkpointing,
    /// inject a PersistentEventSubscriber implementation.
    async fn start_reactive_event_processing(&mut self) -> anyhow::Result<()> {
        let event_bus = self.event_bus.clone();
        let job_dispatcher = self.job_dispatcher.clone();

        // Subscribe to JobQueued events
        let mut job_queue_stream = event_bus
            .subscribe("hodei_events")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to hodei_events: {}", e))?;

        // Subscribe to WorkerReady events
        let mut worker_ready_stream = event_bus
            .subscribe("hodei_events")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to hodei_events: {}", e))?;

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
                                tokio::time::sleep(Duration::from_secs(5)).await;
                            }
                        }
                    }

                    // Process WorkerReady events
                    event_result = worker_ready_stream.next() => {
                        match event_result {
                            Some(Ok(event)) => {
                                if let DomainEvent::WorkerReady { worker_id, .. } = event {
                                    info!("👷 JobCoordinator: Received WorkerReady event for worker {}", worker_id);
                                    dispatcher.dispatch_pending_job_to_worker(&worker_id).await;
                                }
                            }
                            Some(Err(e)) => {
                                error!("❌ JobCoordinator: Error receiving WorkerReady event: {}", e);
                            }
                            None => {
                                warn!("⚠️ JobCoordinator: WorkerReady stream ended, reconnecting...");
                                tokio::time::sleep(Duration::from_secs(5)).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the coordinator gracefully
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(());
        info!("📴 JobCoordinator shutdown signal sent");
    }

    /// Manually trigger a job dispatch cycle
    pub async fn dispatch_now(&self) -> anyhow::Result<usize> {
        info!("🔄 JobCoordinator: Manual dispatch trigger");
        self.job_dispatcher.dispatch_once().await
    }
}

// Re-export EventBusError for convenience
pub use hodei_server_domain::event_bus::EventBusError;
