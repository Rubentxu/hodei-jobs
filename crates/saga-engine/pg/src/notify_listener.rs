//! # PgNotifyListener
//!
//! PostgreSQL LISTEN/NOTIFY implementation for reactive event processing.

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// PostgreSQL notification channel constants
pub const CHANNEL_SAGA_EVENTS: &str = "saga_events";
pub const CHANNEL_SAGA_TIMERS: &str = "saga_timers";
pub const CHANNEL_SAGA_SIGNALS: &str = "saga_signals";
pub const CHANNEL_SAGA_SNAPSHOTS: &str = "saga_snapshots";

/// Trait for notification listener (type-erased using Arc for thread-safe interior mutability)
#[async_trait]
pub trait NotifyListener: Send + Sync {
    fn subscribe(&self, channel: &str) -> NotificationReceiver;
    async fn listen(&self, channel: &str) -> Result<(), sqlx::Error>;
    async fn listen_multiple(&self, channels: &[&'static str]) -> Result<(), sqlx::Error>;
    async fn start(&self) -> Result<(), sqlx::Error>;
    fn stop(&self);
    fn channels(&self) -> Vec<String>;
    fn is_running(&self) -> bool;
}

/// Receiver for notifications
#[derive(Debug)]
pub struct NotificationReceiver {
    pub rx: mpsc::UnboundedReceiver<String>,
}

impl NotificationReceiver {
    pub async fn recv(&mut self) -> Option<String> {
        self.rx.recv().await
    }
}

/// Dispatch notification to all subscribers (standalone function)
fn dispatch_notification(
    subscriptions: &Arc<Mutex<Vec<(String, mpsc::UnboundedSender<String>)>>>,
    channel: &str,
    payload: &str,
) {
    let subs = subscriptions.blocking_lock();

    for (ch, sender) in subs.iter() {
        if ch == channel {
            if sender.send(payload.to_string()).is_err() {
                warn!("Subscriber dropped for channel: {}", channel);
            }
        }
    }
}

/// PostgreSQL NOTIFY listener for reactive event processing
#[derive(Debug)]
pub struct PgNotifyListener {
    /// sqlx PostgreSQL listener (wrapped in Mutex for interior mutability)
    listener: Arc<Mutex<sqlx::postgres::PgListener>>,
    /// Channel subscriptions: channel -> list of senders (using Arc<Mutex> for thread-safety)
    subscriptions: Arc<Mutex<Vec<(String, mpsc::UnboundedSender<String>)>>>,
    /// Running flag for shutdown
    running: Arc<AtomicBool>,
    /// Listen channels
    channels: Arc<Mutex<Vec<String>>>,
}

impl PgNotifyListener {
    /// Create a new PgNotifyListener
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        info!("Connecting PgNotifyListener to PostgreSQL...");
        let listener = sqlx::postgres::PgListener::connect(database_url).await?;
        info!("PgNotifyListener connected to PostgreSQL");

        Ok(Self {
            listener: Arc::new(Mutex::new(listener)),
            subscriptions: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            channels: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Stop the listener
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("PgNotifyListener stopped");
    }

    /// Check if listener is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get list of listening channels
    pub async fn channels(&self) -> Vec<String> {
        self.channels.lock().await.clone()
    }
}

impl Drop for PgNotifyListener {
    fn drop(&mut self) {
        self.stop();
    }
}

#[async_trait]
impl NotifyListener for PgNotifyListener {
    /// Subscribe to a notification channel
    fn subscribe(&self, channel: &str) -> NotificationReceiver {
        let (sender, receiver) = mpsc::unbounded_channel::<String>();

        let mut subs = self.subscriptions.blocking_lock();
        subs.push((channel.to_string(), sender));

        debug!("Subscribed to channel: {}", channel);

        NotificationReceiver { rx: receiver }
    }

    /// Listen to a channel (server-side)
    async fn listen(&self, channel: &str) -> Result<(), sqlx::Error> {
        let mut listener = self.listener.lock().await;
        listener.listen(channel).await?;
        self.channels.lock().await.push(channel.to_string());
        info!("LISTEN on channel: {}", channel);
        Ok(())
    }

    /// Listen to multiple channels
    async fn listen_multiple(&self, channels: &[&'static str]) -> Result<(), sqlx::Error> {
        for channel in channels {
            self.listen(channel).await?;
        }
        Ok(())
    }

    /// Start listening for notifications
    async fn start(&self) -> Result<(), sqlx::Error> {
        if self.running.load(Ordering::SeqCst) {
            warn!("PgNotifyListener already running");
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);

        if self.channels.lock().await.is_empty() {
            self.listen_multiple(&[
                CHANNEL_SAGA_EVENTS,
                CHANNEL_SAGA_TIMERS,
                CHANNEL_SAGA_SIGNALS,
                CHANNEL_SAGA_SNAPSHOTS,
            ])
            .await?;
        }

        info!(
            "PgNotifyListener started, listening on {} channels",
            self.channels.lock().await.len()
        );

        // Clone arcs for the background task
        let running = Arc::clone(&self.running);
        let subscriptions = Arc::clone(&self.subscriptions);
        let listener = Arc::clone(&self.listener);

        // Run notification loop in background
        tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                let mut listener_guard = listener.lock().await;
                match listener_guard.recv().await {
                    Ok(notification) => {
                        let payload = notification.payload().to_owned();
                        let channel = notification.channel().to_string();
                        drop(listener_guard); // Release lock before dispatching

                        dispatch_notification(&subscriptions, &channel, &payload);
                    }
                    Err(e) => {
                        error!("Error receiving notification: {:?}", e);
                        drop(listener_guard); // Release lock before sleeping
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the listener
    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("PgNotifyListener stopped");
    }

    /// Get list of listening channels
    fn channels(&self) -> Vec<String> {
        self.channels.blocking_lock().clone()
    }

    /// Check if listener is running
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_notification_receiver_create() {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let receiver = NotificationReceiver { rx };

        // Send a test message
        tx.send("test payload".to_string()).unwrap();

        // Receive it
        let received = receiver.rx.recv().await;
        assert_eq!(received, Some("test payload".to_string()));
    }

    #[tokio::test]
    async fn test_channel_constants() {
        assert_eq!(CHANNEL_SAGA_EVENTS, "saga_events");
        assert_eq!(CHANNEL_SAGA_TIMERS, "saga_timers");
        assert_eq!(CHANNEL_SAGA_SIGNALS, "saga_signals");
        assert_eq!(CHANNEL_SAGA_SNAPSHOTS, "saga_snapshots");
    }

    #[tokio::test]
    async fn test_pg_notify_listener_creation() {
        // This would require a real database connection
        // Skip in unit tests
    }
}
