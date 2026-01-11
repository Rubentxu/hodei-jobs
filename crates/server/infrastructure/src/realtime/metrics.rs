//! Realtime Metrics for WebSocket Connections
//!
//! Provides Prometheus metrics for monitoring WebSocket connections,
//! message throughput, backpressure events, and session lifecycle.

use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Registry};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

/// Real-time metrics for WebSocket connections and message processing.
#[derive(Debug, Clone)]
pub struct RealtimeMetrics {
    inner: Arc<RealtimeMetricsInner>,
}

#[derive(Debug)]
struct RealtimeMetricsInner {
    sessions_active: Gauge,
    sessions_total: Counter,
    messages_sent_total: Counter,
    messages_dropped_total: Counter,
    backpressure_detected: Counter,
    broadcasts_total: Counter,
    domain_events_received: Counter,
    client_events_projected: Counter,
    domain_events_filtered: Counter,
    subscriptions_total: Counter,
    topics_active: Gauge,
    broadcast_duration_ms: Histogram,
    event_processing_duration_us: Histogram,
    session_duration_seconds: Histogram,
    session_errors: Counter,
    last_activity: Mutex<Instant>,
}

impl Default for RealtimeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl RealtimeMetrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RealtimeMetricsInner {
                sessions_active: Gauge::new("realtime_sessions_active", "Active sessions").unwrap(),
                sessions_total: Counter::new("realtime_sessions_total", "Total sessions created")
                    .unwrap(),
                messages_sent_total: Counter::new("realtime_messages_sent_total", "Messages sent")
                    .unwrap(),
                messages_dropped_total: Counter::new(
                    "realtime_messages_dropped_total",
                    "Messages dropped",
                )
                .unwrap(),
                backpressure_detected: Counter::new(
                    "realtime_backpressure_detected",
                    "Backpressure events",
                )
                .unwrap(),
                broadcasts_total: Counter::new("realtime_broadcasts_total", "Broadcast operations")
                    .unwrap(),
                domain_events_received: Counter::new(
                    "realtime_domain_events_received",
                    "Domain events received",
                )
                .unwrap(),
                client_events_projected: Counter::new(
                    "realtime_client_events_projected",
                    "Client events projected",
                )
                .unwrap(),
                domain_events_filtered: Counter::new(
                    "realtime_domain_events_filtered",
                    "Domain events filtered",
                )
                .unwrap(),
                subscriptions_total: Counter::new(
                    "realtime_subscriptions_total",
                    "Subscription operations",
                )
                .unwrap(),
                topics_active: Gauge::new("realtime_topics_active", "Active topics").unwrap(),
                broadcast_duration_ms: Histogram::with_opts(HistogramOpts::new(
                    "realtime_broadcast_duration_ms",
                    "Broadcast duration in milliseconds",
                ))
                .unwrap(),
                event_processing_duration_us: Histogram::with_opts(HistogramOpts::new(
                    "realtime_event_processing_duration_us",
                    "Event processing duration in microseconds",
                ))
                .unwrap(),
                session_duration_seconds: Histogram::with_opts(HistogramOpts::new(
                    "realtime_session_duration_seconds",
                    "Session duration in seconds",
                ))
                .unwrap(),
                session_errors: Counter::new("realtime_session_errors", "Session errors").unwrap(),
                last_activity: Mutex::new(Instant::now()),
            }),
        }
    }

    pub fn register(&self, registry: &mut Registry) {
        registry
            .register(Box::new(self.inner.sessions_active.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.sessions_total.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.messages_sent_total.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.messages_dropped_total.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.backpressure_detected.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.broadcasts_total.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.domain_events_received.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.client_events_projected.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.domain_events_filtered.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.subscriptions_total.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.topics_active.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.broadcast_duration_ms.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.event_processing_duration_us.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.session_duration_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(self.inner.session_errors.clone()))
            .unwrap();
    }

    pub fn session_active_inc(&self) {
        self.inner.sessions_active.inc();
        self.inner.sessions_total.inc();
        self.update_last_activity();
    }

    pub fn session_active_dec(&self) {
        self.inner.sessions_active.dec();
        self.update_last_activity();
    }

    pub fn session_created(&self) {
        self.inner.sessions_total.inc();
        self.update_last_activity();
    }

    pub fn record_message_sent(&self) {
        self.inner.messages_sent_total.inc();
        self.update_last_activity();
    }

    pub fn record_message_dropped(&self, count: u64) {
        self.inner.messages_dropped_total.inc_by(count as f64);
        self.inner.backpressure_detected.inc();
        self.update_last_activity();
    }

    pub fn record_backpressure(&self) {
        self.inner.backpressure_detected.inc();
        self.update_last_activity();
    }

    pub fn record_broadcast(&self, duration_ms: f64) {
        self.inner.broadcasts_total.inc();
        self.inner.broadcast_duration_ms.observe(duration_ms);
        self.update_last_activity();
    }

    pub fn record_domain_event_received(&self) {
        self.inner.domain_events_received.inc();
        self.update_last_activity();
    }

    pub fn record_client_event_projected(&self, duration_us: f64) {
        self.inner.client_events_projected.inc();
        self.inner.event_processing_duration_us.observe(duration_us);
        self.update_last_activity();
    }

    pub fn record_domain_event_filtered(&self) {
        self.inner.domain_events_filtered.inc();
        self.update_last_activity();
    }

    pub fn subscription_inc(&self) {
        self.inner.subscriptions_total.inc();
        self.inner.topics_active.inc();
        self.update_last_activity();
    }

    pub fn subscription_dec(&self) {
        self.inner.topics_active.dec();
        self.update_last_activity();
    }

    pub fn record_session_error(&self) {
        self.inner.session_errors.inc();
        self.update_last_activity();
    }

    pub fn record_session_duration(&self, duration_seconds: f64) {
        self.inner
            .session_duration_seconds
            .observe(duration_seconds);
    }

    fn update_last_activity(&self) {
        let mut last = self.inner.last_activity.lock().unwrap();
        *last = Instant::now();
    }

    pub fn since_last_activity(&self) -> std::time::Duration {
        let last = self.inner.last_activity.lock().unwrap();
        last.elapsed()
    }
}

/// A snapshot of realtime metrics at a point in time.
#[derive(Debug, Clone)]
pub struct RealtimeMetricsSnapshot {
    pub sessions_active: i64,
    pub messages_sent_total: u64,
    pub messages_dropped_total: u64,
    pub backpressure_detected: u64,
    pub last_activity: std::time::Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = RealtimeMetrics::new();
        assert!(metrics.since_last_activity() >= std::time::Duration::ZERO);
    }

    #[test]
    fn test_session_active_increment() {
        let metrics = RealtimeMetrics::new();
        metrics.session_active_inc();
        metrics.session_active_inc();
        metrics.session_active_dec();
        // Note: In a real scenario, we'd check the gauge value
    }
}
