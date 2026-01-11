//! Connection Manager for WebSocket Sessions

use crate::realtime::metrics::RealtimeMetrics;
use crate::realtime::session::{Session, SessionId};
use dashmap::DashMap;
use hodei_shared::realtime::messages::ClientEvent;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, warn};

#[derive(Debug, thiserror::Error)]
pub enum BroadcastError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("All sessions closed")]
    AllSessionsClosed,

    #[error("Partial broadcast: {0} failed of {1} total")]
    Partial(usize, usize),
}

#[derive(Debug, Clone)]
pub struct ConnectionManagerMetrics {
    pub active_sessions: usize,
    pub active_topics: usize,
    pub total_subscriptions: usize,
}

#[derive(Debug)]
pub struct ConnectionManager {
    sessions: DashMap<SessionId, Arc<Session>>,
    subscriptions: DashMap<String, dashmap::DashSet<SessionId>>,
    metrics: Arc<RealtimeMetrics>,
    shutdown_rx: Option<mpsc::Receiver<()>>,
}

impl ConnectionManager {
    pub fn new(metrics: Arc<RealtimeMetrics>, shutdown_rx: Option<mpsc::Receiver<()>>) -> Self {
        Self {
            sessions: DashMap::new(),
            subscriptions: DashMap::new(),
            metrics,
            shutdown_rx,
        }
    }

    pub fn with_metrics(metrics: Arc<RealtimeMetrics>) -> Self {
        Self::new(metrics, None)
    }

    pub async fn register_session(&self, session: Arc<Session>) {
        let session_id = session.id().to_string();
        self.sessions.insert(session_id.clone(), session.clone());
        self.metrics.session_active_inc();
        debug!(session_id = %session_id, "Session registered");
    }

    pub async fn unregister_session(&self, session_id: &str) -> Option<Arc<Session>> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            for topic in session.subscribed_topics().iter() {
                if let Some(mut topic_subs) = self.subscriptions.get_mut(topic) {
                    topic_subs.remove(session_id);
                    self.metrics.subscription_dec();
                }
            }
            self.metrics.session_active_dec();
            debug!(session_id = %session_id, "Session unregistered");
            return Some(session);
        }
        None
    }

    pub async fn subscribe(&self, session_id: &str, topic: String) {
        if !self.sessions.contains_key(session_id) {
            warn!(session_id = %session_id, "Session not found for subscription");
            return;
        }

        let mut topic_subs = self
            .subscriptions
            .entry(topic.clone())
            .or_insert_with(|| dashmap::DashSet::new());

        topic_subs.insert(session_id.to_string());

        if let Some(session) = self.sessions.get(session_id) {
            session.subscribe(&topic);
        }

        self.metrics.subscription_inc();
        debug!(session_id = %session_id, topic = %topic, "Session subscribed to topic");
    }

    pub async fn unsubscribe(&self, session_id: &str, topic: &str) {
        if let Some(mut topic_subs) = self.subscriptions.get_mut(topic) {
            topic_subs.remove(session_id);
            self.metrics.subscription_dec();
        }

        if let Some(session) = self.sessions.get(session_id) {
            session.unsubscribe(topic);
        }

        debug!(session_id = %session_id, topic = %topic, "Session unsubscribed from topic");
    }

    pub async fn broadcast(
        &self,
        event: &ClientEvent,
        topics: &[String],
    ) -> Result<(), BroadcastError> {
        let message = serde_json::to_string(event)
            .map_err(|e| BroadcastError::Serialization(e.to_string()))?;

        let mut success_count = 0usize;
        let mut fail_count = 0usize;

        let mut target_sessions: Vec<SessionId> = Vec::new();

        for topic in topics {
            if let Some(topic_subs) = self.subscriptions.get(topic) {
                for session_id in topic_subs.iter() {
                    let session_id_str = session_id.clone();
                    if !target_sessions.contains(&session_id_str) {
                        target_sessions.push(session_id_str);
                    }
                }
            }
        }

        for session_id in &target_sessions {
            if let Some(session) = self.sessions.get(session_id) {
                match session.send_message(message.clone()).await {
                    Ok(()) => success_count += 1,
                    Err(crate::realtime::session::SessionError::Closed) => {
                        fail_count += 1;
                        self.sessions.remove(session_id);
                        self.metrics.session_active_dec();
                    }
                    Err(crate::realtime::session::SessionError::Backpressure) => {
                        fail_count += 1;
                        warn!(session_id = %session_id, "Backpressure during broadcast");
                    }
                    Err(crate::realtime::session::SessionError::Serialization(e)) => {
                        return Err(BroadcastError::Serialization(e));
                    }
                }
            }
        }

        if fail_count == 0 {
            Ok(())
        } else if success_count == 0 {
            Err(BroadcastError::AllSessionsClosed)
        } else {
            Err(BroadcastError::Partial(
                fail_count,
                success_count + fail_count,
            ))
        }
    }

    pub async fn broadcast_all(&self, event: &ClientEvent) -> Result<(), BroadcastError> {
        let message = serde_json::to_string(event)
            .map_err(|e| BroadcastError::Serialization(e.to_string()))?;

        let mut success_count = 0usize;
        let mut fail_count = 0usize;

        for entry in self.sessions.iter() {
            let session = entry.value();
            match session.send_message(message.clone()).await {
                Ok(()) => success_count += 1,
                Err(crate::realtime::session::SessionError::Closed) => {
                    fail_count += 1;
                    self.metrics.session_active_dec();
                }
                Err(crate::realtime::session::SessionError::Backpressure) => {
                    fail_count += 1;
                }
                Err(crate::realtime::session::SessionError::Serialization(e)) => {
                    return Err(BroadcastError::Serialization(e));
                }
            }
        }

        if fail_count == 0 {
            Ok(())
        } else if success_count == 0 {
            Err(BroadcastError::AllSessionsClosed)
        } else {
            Err(BroadcastError::Partial(
                fail_count,
                success_count + fail_count,
            ))
        }
    }

    pub fn get_session(&self, session_id: &str) -> Option<Arc<Session>> {
        self.sessions.get(session_id).map(|entry| entry.clone())
    }

    pub fn active_sessions_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn active_topics_count(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn total_subscriptions(&self) -> usize {
        self.subscriptions.iter().map(|entry| entry.len()).sum()
    }

    pub fn metrics(&self) -> ConnectionManagerMetrics {
        ConnectionManagerMetrics {
            active_sessions: self.sessions.len(),
            active_topics: self.subscriptions.len(),
            total_subscriptions: self.subscriptions.iter().map(|entry| entry.len()).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::session::SESSION_CHANNEL_CAPACITY;
    use chrono::Utc;
    use hodei_shared::realtime::messages::ClientEventPayload;
    use tokio::sync::mpsc;

    fn create_test_session(
        id: &str,
        metrics: Arc<RealtimeMetrics>,
    ) -> (Arc<Session>, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(SESSION_CHANNEL_CAPACITY);
        let session = Arc::new(Session::new(id.to_string(), tx, metrics));
        (session, rx)
    }

    fn create_test_event() -> ClientEvent {
        ClientEvent {
            event_type: "job.status_changed".to_string(),
            event_version: 1,
            aggregate_id: "job-123".to_string(),
            payload: ClientEventPayload::JobStatusChanged {
                job_id: "job-123".to_string(),
                old_status: "QUEUED".to_string(),
                new_status: "RUNNING".to_string(),
                timestamp: Utc::now().timestamp_millis(),
            },
            occurred_at: Utc::now().timestamp_millis(),
        }
    }

    #[tokio::test]
    async fn test_register_unregister_session() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let manager = ConnectionManager::with_metrics(metrics.clone());

        let (session, _rx) = create_test_session("sess-1", metrics.clone());

        assert_eq!(manager.active_sessions_count(), 0);
        manager.register_session(session.clone()).await;
        assert_eq!(manager.active_sessions_count(), 1);
        assert!(manager.get_session("sess-1").is_some());

        let removed = manager.unregister_session("sess-1").await;
        assert!(removed.is_some());
        assert_eq!(manager.active_sessions_count(), 0);
    }

    #[tokio::test]
    async fn test_subscribe_unsubscribe() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let manager = ConnectionManager::with_metrics(metrics.clone());

        let (session1, _rx) = create_test_session("sess-1", metrics.clone());
        let (session2, _rx) = create_test_session("sess-2", metrics.clone());

        manager.register_session(session1.clone()).await;
        manager.register_session(session2.clone()).await;

        manager.subscribe("sess-1", "jobs:all".to_string()).await;
        manager.subscribe("sess-1", "workers:all".to_string()).await;
        manager.subscribe("sess-2", "jobs:all".to_string()).await;

        assert_eq!(manager.active_topics_count(), 2);
        assert_eq!(manager.total_subscriptions(), 3);

        manager.unsubscribe("sess-1", "workers:all").await;
        assert_eq!(manager.total_subscriptions(), 2);
    }

    #[tokio::test]
    async fn test_broadcast_to_topics() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let manager = ConnectionManager::with_metrics(metrics.clone());

        let (session1, mut rx1) = create_test_session("sess-1", metrics.clone());
        let (session2, mut rx2) = create_test_session("sess-2", metrics.clone());

        manager.register_session(session1.clone()).await;
        manager.register_session(session2.clone()).await;

        manager.subscribe("sess-1", "jobs:all".to_string()).await;
        manager.subscribe("sess-2", "workers:all".to_string()).await;

        let event = create_test_event();
        manager
            .broadcast(&event, &["jobs:all".to_string()])
            .await
            .unwrap();

        let received1 = tokio::time::timeout(Duration::from_millis(100), rx1.recv()).await;
        let received2 = tokio::time::timeout(Duration::from_millis(100), rx2.recv()).await;

        assert!(received1.is_ok());
        assert!(received2.is_err());
    }

    #[tokio::test]
    async fn test_broadcast_all() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let manager = ConnectionManager::with_metrics(metrics.clone());

        let (session1, mut rx1) = create_test_session("sess-1", metrics.clone());
        let (session2, mut rx2) = create_test_session("sess-2", metrics.clone());

        manager.register_session(session1.clone()).await;
        manager.register_session(session2.clone()).await;

        let event = create_test_event();
        manager.broadcast_all(&event).await.unwrap();

        let received1 = tokio::time::timeout(Duration::from_millis(100), rx1.recv()).await;
        let received2 = tokio::time::timeout(Duration::from_millis(100), rx2.recv()).await;

        assert!(received1.is_ok());
        assert!(received2.is_ok());
    }
}
