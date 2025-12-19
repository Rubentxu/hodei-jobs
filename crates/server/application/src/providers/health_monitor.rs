//! Provider Health Monitor
//!
//! Monitors provider health proactively and publishes events for status changes.

use chrono::Utc;
use futures::StreamExt;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::shared_kernel::{ProviderId, ProviderStatus, Result};
use hodei_server_domain::workers::{ProviderError, WorkerProvider};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{error, info, warn};

/// Trait for recording provider health metrics
#[async_trait::async_trait]
pub trait HealthMetricsRecorder: Send + Sync {
    /// Record health check duration
    async fn record_health_check_duration(
        &self,
        provider_id: &ProviderId,
        provider_type: &str,
        duration_secs: f64,
    );

    /// Record successful health check
    async fn record_health_check_success(&self, provider_id: &ProviderId, provider_type: &str);

    /// Record failed health check
    async fn record_health_check_error(
        &self,
        provider_id: &ProviderId,
        provider_type: &str,
        error_type: &str,
    );

    /// Record total health check attempt
    async fn record_health_check_total(&self, provider_id: &ProviderId, provider_type: &str);
}

/// No-op metrics recorder (for testing or when metrics are not needed)
pub struct NoopMetricsRecorder;

#[async_trait::async_trait]
impl HealthMetricsRecorder for NoopMetricsRecorder {
    async fn record_health_check_duration(
        &self,
        _provider_id: &ProviderId,
        _provider_type: &str,
        _duration_secs: f64,
    ) {
    }

    async fn record_health_check_success(&self, _provider_id: &ProviderId, _provider_type: &str) {}

    async fn record_health_check_error(
        &self,
        _provider_id: &ProviderId,
        _provider_type: &str,
        _error_type: &str,
    ) {
    }

    async fn record_health_check_total(&self, _provider_id: &ProviderId, _provider_type: &str) {}
}

/// Health metrics for a provider
#[derive(Debug, Clone)]
struct ProviderHealthMetrics {
    /// Total health checks performed
    total_checks: u64,
    /// Successful health checks
    successful_checks: u64,
    /// Failed health checks
    failed_checks: u64,
    /// Response time in milliseconds
    response_times: Vec<u64>,
    /// Last health check timestamp
    last_check: Option<Instant>,
    /// Current health status
    current_status: ProviderStatus,
    /// How long the provider has been in current status
    status_duration: Duration,
    /// When the status last changed
    status_changed_at: Instant,
}

/// Provider Health Monitor
///
/// Monitors provider health with configurable intervals and publishes events
/// when health status changes.
pub struct ProviderHealthMonitor<E, M>
where
    E: EventBus + 'static,
    M: HealthMetricsRecorder + 'static,
{
    event_bus: Arc<E>,
    providers: Vec<Arc<dyn WorkerProvider>>,
    health_check_interval: Duration,
    alert_threshold: Duration,
    provider_metrics: HashMap<ProviderId, ProviderHealthMetrics>,
    metrics_recorder: Arc<M>,
    monitoring_task: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

impl<E, M> ProviderHealthMonitor<E, M>
where
    E: EventBus,
    M: HealthMetricsRecorder,
{
    /// Create a new ProviderHealthMonitor
    ///
    /// # Arguments
    /// * `event_bus` - Event bus for publishing health events
    /// * `providers` - List of providers to monitor
    /// * `health_check_interval` - How often to perform health checks (default: 30s)
    /// * `alert_threshold` - Duration before alerting on unhealthy status (default: 2min)
    /// * `metrics_recorder` - Metrics recorder for recording health check metrics
    pub fn new(
        event_bus: Arc<E>,
        providers: Vec<Arc<dyn WorkerProvider>>,
        health_check_interval: Duration,
        alert_threshold: Duration,
        metrics_recorder: Arc<M>,
    ) -> Self {
        let provider_metrics = HashMap::new();
        Self {
            event_bus,
            providers,
            health_check_interval,
            alert_threshold,
            provider_metrics,
            metrics_recorder,
            monitoring_task: None,
            shutdown_tx: None,
        }
    }

    /// Start health monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!(
            "Starting provider health monitoring with interval {:?}",
            self.health_check_interval
        );

        // Initialize metrics for all providers
        for provider in &self.providers {
            let provider_id = provider.provider_id().clone();
            let metrics = ProviderHealthMetrics {
                total_checks: 0,
                successful_checks: 0,
                failed_checks: 0,
                response_times: Vec::new(),
                last_check: None,
                current_status: ProviderStatus::Active, // Default to Active
                status_duration: Duration::from_secs(0),
                status_changed_at: Instant::now(),
            };
            self.provider_metrics.insert(provider_id, metrics);
        }

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Spawn monitoring task
        let event_bus = self.event_bus.clone();
        let providers = self.providers.clone();
        let health_check_interval = self.health_check_interval;
        let alert_threshold = self.alert_threshold;
        let metrics_recorder = self.metrics_recorder.clone();

        // We need to track metrics across iterations, so we'll use Arc<Mutex<HashMap>>
        let metrics_map = Arc::new(tokio::sync::Mutex::new(self.provider_metrics.clone()));

        let task = tokio::spawn(async move {
            let mut interval = interval(health_check_interval);
            let mut consecutive_errors: HashMap<ProviderId, u32> = HashMap::new();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Perform health checks for all providers
                        for provider in &providers {
                            let provider_id = provider.provider_id().clone();
                            let provider_type = provider.provider_type().to_string();
                            let start = Instant::now();

                            // Record total health check attempt
                            metrics_recorder
                                .record_health_check_total(&provider_id, &provider_type)
                                .await;

                            // Perform health check
                            match provider.health_check().await {
                                Ok(health_status) => {
                                    let response_time = start.elapsed().as_secs_f64();
                                    consecutive_errors.insert(provider_id.clone(), 0);

                                    // Record successful health check
                                    metrics_recorder
                                        .record_health_check_success(&provider_id, &provider_type)
                                        .await;

                                    // Record response time
                                    metrics_recorder
                                        .record_health_check_duration(&provider_id, &provider_type, response_time)
                                        .await;

                                    // Update metrics
                                    {
                                        let mut metrics = metrics_map.lock().await;
                                        if let Some(m) = metrics.get_mut(&provider_id) {
                                            m.total_checks += 1;
                                            m.successful_checks += 1;
                                            m.response_times.push(response_time as u64);
                                            if m.response_times.len() > 100 {
                                                m.response_times.remove(0);
                                            }
                                            m.last_check = Some(Instant::now());

                                            // Check if status changed
                                            let new_status = match health_status {
                                                hodei_server_domain::workers::HealthStatus::Healthy => ProviderStatus::Active,
                                                hodei_server_domain::workers::HealthStatus::Degraded { .. } => ProviderStatus::Overloaded,
                                                hodei_server_domain::workers::HealthStatus::Unhealthy { .. } => ProviderStatus::Unhealthy,
                                                hodei_server_domain::workers::HealthStatus::Unknown => ProviderStatus::Disabled,
                                            };

                                            if m.current_status != new_status {
                                                let old_status = m.current_status.clone();
                                                let status_for_log = old_status.clone();
                                                m.current_status = new_status.clone();
                                                m.status_changed_at = Instant::now();

                                                // Publish health changed event
                                                let event = DomainEvent::ProviderHealthChanged {
                                                    provider_id: provider_id.clone(),
                                                    old_status,
                                                    new_status,
                                                    occurred_at: Utc::now(),
                                                    correlation_id: Some("health-monitor".to_string()),
                                                    actor: Some("provider-health-monitor".to_string()),
                                                };

                                                if let Err(e) = event_bus.publish(&event).await {
                                                    error!("Failed to publish ProviderHealthChanged event: {}", e);
                                                } else {
                                                    info!(
                                                        "Provider {} health changed: {:?} -> {:?}",
                                                        provider_id, status_for_log, m.current_status
                                                    );

                                                    // If provider recovered, publish ProviderRecovered event
                                                    if m.current_status == ProviderStatus::Active {
                                                        let recovered_event = DomainEvent::ProviderRecovered {
                                                            provider_id: provider_id.clone(),
                                                            previous_status: status_for_log,
                                                            occurred_at: Utc::now(),
                                                            correlation_id: Some("health-monitor".to_string()),
                                                            actor: Some("provider-health-monitor".to_string()),
                                                        };

                                                        if let Err(e) = event_bus.publish(&recovered_event).await {
                                                            error!("Failed to publish ProviderRecovered event: {}", e);
                                                        } else {
                                                            info!("Provider {} recovered", provider_id);
                                                        }
                                                    }
                                                }
                                            }

                                            // Check for alert threshold
                                            if matches!(m.current_status, ProviderStatus::Unhealthy)
                                                && m.status_changed_at.elapsed() > alert_threshold
                                            {
                                                warn!(
                                                    "Provider {} has been unhealthy for {:?}, triggering alert",
                                                    provider_id, m.status_changed_at.elapsed()
                                                );
                                                // TODO: Publish alert event
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Health check failed for provider {}: {}", provider_id, e);
                                    consecutive_errors.insert(provider_id.clone(),
                                        consecutive_errors.get(&provider_id).unwrap_or(&0) + 1);

                                    // Record failed health check with error type
                                    let error_type = match e {
                                        ProviderError::ConnectionFailed(_) => "connection_failed",
                                        ProviderError::Timeout(_) => "timeout",
                                        ProviderError::AuthenticationFailed(_) => "auth_failed",
                                        ProviderError::ProviderSpecific(_) => "provider_specific",
                                        _ => "unknown_error",
                                    };
                                    metrics_recorder
                                        .record_health_check_error(&provider_id, &provider_type, error_type)
                                        .await;

                                    // Update failed checks metric
                                    {
                                        let mut metrics = metrics_map.lock().await;
                                        if let Some(m) = metrics.get_mut(&provider_id) {
                                            m.total_checks += 1;
                                            m.failed_checks += 1;
                                            m.last_check = Some(Instant::now());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Provider health monitoring shutting down");
                        break;
                    }
                }
            }
        });

        self.monitoring_task = Some(task);
        self.shutdown_tx = Some(shutdown_tx);

        Ok(())
    }

    /// Stop health monitoring
    pub async fn stop_monitoring(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(()).await;
        }

        if let Some(task) = self.monitoring_task.take() {
            task.await.map_err(|e| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Error waiting for monitoring task: {}", e),
                }
            })?;
        }

        Ok(())
    }

    /// Get health metrics for a specific provider
    pub async fn get_provider_metrics(
        &self,
        provider_id: &ProviderId,
    ) -> Option<&ProviderHealthMetrics> {
        self.provider_metrics.get(provider_id)
    }

    /// Get health metrics for all providers
    pub async fn get_all_metrics(&self) -> &HashMap<ProviderId, ProviderHealthMetrics> {
        &self.provider_metrics
    }

    /// Perform an immediate health check for a specific provider
    pub async fn check_provider_health(
        &self,
        provider_id: &ProviderId,
    ) -> Result<hodei_server_domain::workers::HealthStatus> {
        for provider in &self.providers {
            if provider.provider_id() == provider_id {
                return provider.health_check().await.map_err(|e| {
                    hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                        message: format!("Health check failed: {}", e),
                    }
                });
            }
        }

        Err(
            hodei_server_domain::shared_kernel::DomainError::ProviderNotFound {
                provider_id: provider_id.clone(),
            },
        )
    }
}

impl<E, M> Drop for ProviderHealthMonitor<E, M>
where
    E: EventBus,
    M: HealthMetricsRecorder,
{
    fn drop(&mut self) {
        if self.monitoring_task.is_some() {
            // Best effort shutdown
            if let Some(shutdown_tx) = &self.shutdown_tx {
                let _ = shutdown_tx.blocking_send(());
            }
        }
    }
}

// Mock implementations for testing
#[cfg(test)]
pub mod tests {
    use super::*;
    use async_trait::async_trait;
    use hodei_server_domain::workers::{
        HealthStatus, ProviderCapabilities, ProviderError, ProviderType, WorkerHandle, WorkerSpec,
    };

    pub struct MockEventBus {
        published_events: Arc<tokio::sync::Mutex<Vec<DomainEvent>>>,
    }

    impl MockEventBus {
        pub fn new() -> Self {
            Self {
                published_events: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            }
        }

        pub async fn get_published_events(&self) -> Vec<DomainEvent> {
            let events = self.published_events.lock().await;
            events.clone()
        }
    }

    #[async_trait]
    impl EventBus for MockEventBus {
        async fn publish(&self, event: &DomainEvent) -> std::result::Result<(), EventBusError> {
            let mut events = self.published_events.lock().await;
            events.push(event.clone());
            Ok(())
        }

        async fn subscribe(
            &self,
            _topic: &str,
        ) -> std::result::Result<
            futures::stream::BoxStream<'static, std::result::Result<DomainEvent, EventBusError>>,
            EventBusError,
        > {
            Ok(futures::stream::empty().boxed())
        }
    }

    pub struct MockWorkerProvider {
        provider_id: ProviderId,
        health_status: HealthStatus,
    }

    impl MockWorkerProvider {
        pub fn new(provider_id: ProviderId) -> Self {
            Self {
                provider_id,
                health_status: HealthStatus::Healthy,
            }
        }

        pub fn set_health_status(&mut self, status: HealthStatus) {
            self.health_status = status;
        }
    }

    #[async_trait]
    impl WorkerProvider for MockWorkerProvider {
        fn provider_id(&self) -> &ProviderId {
            &self.provider_id
        }

        fn provider_type(&self) -> ProviderType {
            ProviderType::Docker
        }

        fn capabilities(&self) -> &ProviderCapabilities {
            static CAPABILITIES: std::sync::LazyLock<ProviderCapabilities> =
                std::sync::LazyLock::new(|| ProviderCapabilities::default());
            &CAPABILITIES
        }

        async fn create_worker(
            &self,
            _spec: &WorkerSpec,
        ) -> std::result::Result<WorkerHandle, ProviderError> {
            unimplemented!()
        }

        async fn get_worker_status(
            &self,
            _handle: &WorkerHandle,
        ) -> std::result::Result<hodei_server_domain::shared_kernel::WorkerState, ProviderError>
        {
            unimplemented!()
        }

        async fn destroy_worker(
            &self,
            _handle: &WorkerHandle,
        ) -> std::result::Result<(), ProviderError> {
            unimplemented!()
        }

        async fn get_worker_logs(
            &self,
            _handle: &WorkerHandle,
            _tail: Option<u32>,
        ) -> std::result::Result<Vec<hodei_server_domain::workers::LogEntry>, ProviderError>
        {
            unimplemented!()
        }

        async fn health_check(&self) -> std::result::Result<HealthStatus, ProviderError> {
            Ok(self.health_status.clone())
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use hodei_server_domain::shared_kernel::ProviderId;

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let event_bus = Arc::new(crate::tests::MockEventBus::new());
        let provider_id = ProviderId::new();
        let provider = Arc::new(crate::tests::MockWorkerProvider::new(provider_id.clone()));
        let metrics_recorder = Arc::new(NoopMetricsRecorder);

        let monitor = ProviderHealthMonitor::new(
            event_bus,
            vec![provider],
            Duration::from_secs(30),
            Duration::from_secs(120),
            metrics_recorder,
        );

        assert_eq!(monitor.providers.len(), 1);
        assert_eq!(monitor.health_check_interval, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_health_monitor_start_stop() {
        let event_bus = Arc::new(crate::tests::MockEventBus::new());
        let provider_id = ProviderId::new();
        let provider = Arc::new(crate::tests::MockWorkerProvider::new(provider_id.clone()));
        let metrics_recorder = Arc::new(NoopMetricsRecorder);

        let mut monitor = ProviderHealthMonitor::new(
            event_bus,
            vec![provider],
            Duration::from_secs(5),
            Duration::from_secs(120),
            metrics_recorder,
        );

        monitor.start_monitoring().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        monitor.stop_monitoring().await.unwrap();
    }
}
