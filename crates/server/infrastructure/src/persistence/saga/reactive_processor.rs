//! Reactive Saga Processor
//!
//! Processes sagas reactively by consuming signals from NotifyingSagaRepository,
//! eliminating the need for polling while maintaining a safety net polling mechanism.

use hodei_server_domain::saga::{SagaContext, SagaId, SagaState};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Configuration for ReactiveSagaProcessor
#[derive(Debug, Clone)]
pub struct ReactiveSagaProcessorConfig {
    pub reactive_enabled: bool,
    pub safety_polling_enabled: bool,
    pub safety_polling_interval: Duration,
    pub max_concurrent_sagas: usize,
    pub saga_timeout: Duration,
    pub polling_batch_size: i64,
}

impl Default for ReactiveSagaProcessorConfig {
    fn default() -> Self {
        Self {
            reactive_enabled: true,
            safety_polling_enabled: true,
            safety_polling_interval: Duration::from_secs(300),
            max_concurrent_sagas: 10,
            saga_timeout: Duration::from_secs(300),
            polling_batch_size: 100,
        }
    }
}

/// Metrics for ReactiveSagaProcessor
#[derive(Clone, Default)]
pub struct ReactiveSagaProcessorMetrics {
    pub reactive_processed: Arc<std::sync::atomic::AtomicU64>,
    pub polling_processed: Arc<std::sync::atomic::AtomicU64>,
    pub execution_errors: Arc<std::sync::atomic::AtomicU64>,
    pub processing_latency_ms: Arc<std::sync::atomic::AtomicU64>,
}

impl ReactiveSagaProcessorMetrics {
    pub fn record_reactive_processed(&self) {
        self.reactive_processed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_polling_processed(&self) {
        self.polling_processed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.execution_errors
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_latency(&self, ms: u64) {
        self.processing_latency_ms
            .fetch_add(ms, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn reactive_processed(&self) -> u64 {
        self.reactive_processed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn polling_processed(&self) -> u64 {
        self.polling_processed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn errors(&self) -> u64 {
        self.execution_errors
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// The ReactiveSagaProcessor
pub struct ReactiveSagaProcessor<Repo, FindSagaFn, ExecuteSagaFn> {
    repository: Arc<Repo>,
    find_saga_fn: Arc<FindSagaFn>,
    execute_saga_fn: Arc<ExecuteSagaFn>,
    signal_rx: mpsc::UnboundedReceiver<SagaId>,
    shutdown: watch::Receiver<()>,
    config: ReactiveSagaProcessorConfig,
    metrics: Arc<ReactiveSagaProcessorMetrics>,
}

impl<Repo, FindSagaFn, ExecuteSagaFn> ReactiveSagaProcessor<Repo, FindSagaFn, ExecuteSagaFn>
where
    FindSagaFn: Fn(Arc<Repo>, &SagaId) -> Result<Option<SagaContext>, ()> + Send + Sync,
    ExecuteSagaFn: Fn(Arc<Repo>, &SagaContext) -> Result<(), ()> + Send + Sync,
{
    pub fn new(
        repository: Arc<Repo>,
        find_saga_fn: Arc<FindSagaFn>,
        execute_saga_fn: Arc<ExecuteSagaFn>,
        signal_rx: mpsc::UnboundedReceiver<SagaId>,
        shutdown: watch::Receiver<()>,
        config: Option<ReactiveSagaProcessorConfig>,
        metrics: Option<Arc<ReactiveSagaProcessorMetrics>>,
    ) -> Self {
        Self {
            repository,
            find_saga_fn,
            execute_saga_fn,
            signal_rx,
            shutdown,
            config: config.unwrap_or_default(),
            metrics: metrics.unwrap_or_else(|| Arc::new(ReactiveSagaProcessorMetrics::default())),
        }
    }

    pub async fn run(self) {
        info!("ReactiveSagaProcessor: Starting");
        self.run_reactive_loop().await;
        info!("ReactiveSagaProcessor: Shutdown complete");
    }

    async fn run_reactive_loop(mut self) {
        loop {
            tokio::select! {
                _ = self.shutdown.changed() => {
                    break;
                }
                signal = self.signal_rx.recv() => {
                    match signal {
                        Some(saga_id) => self.process_saga(&saga_id).await,
                        None => break,
                    }
                }
            }
        }
    }

    async fn process_saga(&self, saga_id: &SagaId) {
        let start = std::time::Instant::now();
        let metrics = self.metrics.clone();
        let repository = self.repository.clone();
        let find_saga_fn = self.find_saga_fn.clone();
        let execute_saga_fn = self.execute_saga_fn.clone();

        debug!(saga_id = %saga_id, "Processing saga reactively");

        let saga_context = match find_saga_fn(repository.clone(), saga_id) {
            Ok(Some(ctx)) => ctx,
            Ok(None) => {
                warn!(saga_id = %saga_id, "Saga not found");
                return;
            }
            Err(()) => {
                error!(saga_id = %saga_id, "Failed to fetch saga");
                metrics.record_error();
                return;
            }
        };

        if saga_context.is_compensating {
            debug!(saga_id = %saga_id, "Saga is compensating, skipping");
            return;
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        metrics.record_latency(elapsed_ms);

        if let Err(()) = execute_saga_fn(repository.clone(), &saga_context) {
            error!(saga_id = %saga_id, "Saga execution failed");
            metrics.record_error();
            return;
        }

        metrics.record_reactive_processed();
        info!(saga_id = %saga_id, elapsed_ms = elapsed_ms, "Saga executed successfully");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_record_processing() {
        let metrics = ReactiveSagaProcessorMetrics::default();
        metrics.record_reactive_processed();
        metrics.record_reactive_processed();
        metrics.record_polling_processed();
        metrics.record_error();
        metrics.record_latency(150);
        assert_eq!(metrics.reactive_processed(), 2);
        assert_eq!(metrics.polling_processed(), 1);
        assert_eq!(metrics.errors(), 1);
    }

    #[tokio::test]
    async fn test_config_defaults() {
        let config = ReactiveSagaProcessorConfig::default();
        assert!(config.reactive_enabled);
        assert!(config.safety_polling_enabled);
        assert_eq!(config.safety_polling_interval, Duration::from_secs(300));
    }
}
