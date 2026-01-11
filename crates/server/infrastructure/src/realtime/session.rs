//! WebSocket Session with Backpressure and Batching

use crate::realtime::metrics::RealtimeMetrics;
use hodei_shared::realtime::messages::{ClientEvent, ServerMessage};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{error, warn};

pub const SESSION_CHANNEL_CAPACITY: usize = 1000;
pub const BACKPRESSURE_THRESHOLD: u8 = 80;
const BATCH_FLUSH_INTERVAL_MS: u64 = 200;
const BATCH_MAX_SIZE: usize = 50;

pub type SessionId = String;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Backpressure: channel full")]
    Backpressure,

    #[error("Session closed")]
    Closed,

    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone)]
pub struct SessionMetrics {
    pub id: SessionId,
    pub messages_sent: u64,
    pub messages_dropped: u64,
    pub active_subscriptions: usize,
    pub drop_rate_percent: f64,
    pub session_duration_seconds: f64,
    pub last_backpressure_seconds: Option<f64>,
}

#[derive(Debug)]
pub struct Session {
    id: SessionId,
    tx: mpsc::Sender<String>,
    subscribed_topics: Arc<Mutex<Vec<String>>>,
    metrics: Arc<RealtimeMetrics>,
    messages_sent: Arc<std::sync::atomic::AtomicU64>,
    messages_dropped: Arc<std::sync::atomic::AtomicU64>,
    last_backpressure: Arc<std::sync::atomic::AtomicU64>,
    session_start: Instant,
    batch_buffer: Arc<Mutex<Vec<String>>>,
}

impl Session {
    pub fn new(id: SessionId, tx: mpsc::Sender<String>, metrics: Arc<RealtimeMetrics>) -> Self {
        Self {
            id,
            tx,
            subscribed_topics: Arc::new(Mutex::new(Vec::new())),
            metrics,
            messages_sent: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            messages_dropped: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_backpressure: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            session_start: Instant::now(),
            batch_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn subscribe(&self, topic: &str) {
        let mut topics = self.subscribed_topics.lock().unwrap();
        if !topics.contains(&topic.to_string()) {
            topics.push(topic.to_string());
            self.metrics.subscription_inc();
        }
    }

    pub fn unsubscribe(&self, topic: &str) {
        let mut topics = self.subscribed_topics.lock().unwrap();
        topics.retain(|t| t != topic);
        self.metrics.subscription_dec();
    }

    pub fn is_subscribed(&self, topic: &str) -> bool {
        let topics = self.subscribed_topics.lock().unwrap();
        topics.iter().any(|t| t == topic)
    }

    pub fn subscribed_topics(&self) -> Vec<String> {
        self.subscribed_topics.lock().unwrap().clone()
    }

    pub async fn send_message(&self, message: String) -> Result<(), SessionError> {
        match self.tx.try_send(message) {
            Ok(()) => {
                self.messages_sent
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.metrics.record_message_sent();
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.handle_backpressure().await?;
                Err(SessionError::Backpressure)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.metrics.record_session_error();
                Err(SessionError::Closed)
            }
        }
    }

    async fn handle_backpressure(&self) -> Result<(), SessionError> {
        let now = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
        self.last_backpressure
            .store(now, std::sync::atomic::Ordering::Relaxed);
        self.metrics.record_backpressure();
        warn!(session_id = %self.id, "Backpressure detected on session");
        Ok(())
    }

    pub async fn send_event(&self, event: &ClientEvent) -> Result<(), SessionError> {
        let message =
            serde_json::to_string(event).map_err(|e| SessionError::Serialization(e.to_string()))?;
        self.send_message(message).await
    }

    pub fn metrics(&self) -> SessionMetrics {
        let sent = self
            .messages_sent
            .load(std::sync::atomic::Ordering::Relaxed);
        let dropped = self
            .messages_dropped
            .load(std::sync::atomic::Ordering::Relaxed);
        let total = sent.saturating_add(dropped);

        let drop_rate = if total > 0 {
            (dropped as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let duration = self.session_start.elapsed().as_secs_f64();

        let last_backpressure_secs = {
            let last = self
                .last_backpressure
                .load(std::sync::atomic::Ordering::Relaxed);
            if last > 0 {
                let now = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
                Some((now - last) as f64)
            } else {
                None
            }
        };

        SessionMetrics {
            id: self.id.clone(),
            messages_sent: sent,
            messages_dropped: dropped,
            active_subscriptions: self.subscribed_topics.lock().unwrap().len(),
            drop_rate_percent: drop_rate,
            session_duration_seconds: duration,
            last_backpressure_seconds: last_backpressure_secs,
        }
    }

    pub fn calculate_drop_rate(&self) -> f64 {
        let sent = self
            .messages_sent
            .load(std::sync::atomic::Ordering::Relaxed);
        let dropped = self
            .messages_dropped
            .load(std::sync::atomic::Ordering::Relaxed);
        let total = sent.saturating_add(dropped);

        if total == 0 {
            0.0
        } else {
            (dropped as f64 / total as f64) * 100.0
        }
    }
}

pub async fn session_loop(
    session: Arc<Session>,
    mut rx: mpsc::Receiver<String>,
    ws_tx: mpsc::Sender<String>,
) {
    let mut flush_interval = interval(Duration::from_millis(BATCH_FLUSH_INTERVAL_MS));
    let mut buffer = Vec::with_capacity(BATCH_MAX_SIZE);

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                buffer.push(event);
                if buffer.len() >= BATCH_MAX_SIZE {
                    if let Err(e) = send_batch(&ws_tx, &buffer, &session).await {
                        error!(session_id = %session.id(), "Failed to send batch: {}", e);
                        let dropped = buffer.len() as u64;
                        session.messages_dropped.fetch_add(dropped, std::sync::atomic::Ordering::Relaxed);
                    }
                    buffer.clear();
                }
            }
            _ = flush_interval.tick() => {
                if !buffer.is_empty() {
                    if let Err(e) = send_batch(&ws_tx, &buffer, &session).await {
                        error!(session_id = %session.id(), "Failed to flush batch: {}", e);
                        let dropped = buffer.len() as u64;
                        session.messages_dropped.fetch_add(dropped, std::sync::atomic::Ordering::Relaxed);
                    }
                    buffer.clear();
                }
            }
        }
    }
}

async fn send_batch(
    ws_tx: &mpsc::Sender<String>,
    buffer: &[String],
    session: &Session,
) -> Result<(), SessionError> {
    if buffer.is_empty() {
        return Ok(());
    }

    let events: Vec<ClientEvent> = buffer
        .iter()
        .filter_map(|m| serde_json::from_str(m).ok())
        .collect();

    if events.is_empty() {
        return Ok(());
    }

    let batch_message = ServerMessage::Batch { events };
    let json = serde_json::to_string(&batch_message)
        .map_err(|e| SessionError::Serialization(e.to_string()))?;

    match ws_tx.send(json).await {
        Ok(()) => {
            session
                .messages_sent
                .fetch_add(buffer.len() as u64, std::sync::atomic::Ordering::Relaxed);
            session.metrics.record_message_sent();
            Ok(())
        }
        Err(_) => {
            session
                .messages_dropped
                .fetch_add(buffer.len() as u64, std::sync::atomic::Ordering::Relaxed);
            session.metrics.record_message_dropped(buffer.len() as u64);
            Err(SessionError::Closed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_session_creation() {
        let (tx, _rx) = mpsc::channel(100);
        let metrics = Arc::new(RealtimeMetrics::new());
        let session = Session::new("test-session".to_string(), tx, metrics);

        assert_eq!(session.id(), "test-session");
        assert!(session.subscribed_topics().is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_unsubscribe() {
        let (tx, _rx) = mpsc::channel(100);
        let metrics = Arc::new(RealtimeMetrics::new());
        let session = Session::new("test-session".to_string(), tx, metrics);

        session.subscribe("jobs:all");
        assert!(session.is_subscribed("jobs:all"));
        assert_eq!(session.subscribed_topics().len(), 1);

        session.unsubscribe("jobs:all");
        assert!(!session.is_subscribed("jobs:all"));
        assert!(session.subscribed_topics().is_empty());
    }
}
