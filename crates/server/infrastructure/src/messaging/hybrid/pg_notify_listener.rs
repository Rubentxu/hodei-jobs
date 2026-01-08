//! PgNotifyListener - Shared PostgreSQL LISTEN/NOTIFY wrapper
//!
//! This module provides a reusable PgNotifyListener for both Command and Event outboxes.
//! It handles PostgreSQL notifications using the LISTEN/NOTIFY mechanism.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────┐     ┌─────────────────────┐
//! │   PgNotifyListener  │────▶│  mpsc::Channel      │
//! │                     │     │  (buffer of 100)    │
//! │  - for_commands()   │     └──────────┬──────────┘
//! │  - for_events()     │                │
//! │  - notify()         │                ▼
//! └─────────────────────┘     ┌─────────────────────┐
//!                             │  Application        │
//!                             │  (consume events)   │
//!                             └─────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust
//! let pool = sqlx::PgPool::connect(&url).await?;
//!
//! // For commands
//! let listener = PgNotifyListener::for_commands(&pool).await?;
//!
//! // For events
//! let listener = PgNotifyListener::for_events(&pool).await?;
//!
//! // Receive notifications
//! while let Some(notification) = listener.recv().await {
//!     // Process notification
//! }
//! ```

use async_trait::async_trait;
use sqlx::postgres::{PgListener, PgNotification};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::spawn;
use tracing::{debug, error, trace};

/// Channel capacity for notifications
const CHANNEL_CAPACITY: usize = 100;

/// Wrapper for PostgreSQL LISTEN/NOTIFY mechanism.
///
/// This struct provides a clean interface for receiving PostgreSQL notifications
/// while handling connection issues automatically.
#[derive(Debug, Clone)]
pub struct PgNotifyListener {
    /// The underlying PgListener
    listener: PgListener,
    /// Channel sender for notifications
    tx: mpsc::Sender<PgNotification>,
    /// Receiver for consuming notifications (kept alive)
    _rx: mpsc::Receiver<PgNotification>,
    /// The channel name this listener is subscribed to
    channel: String,
}

impl PgNotifyListener {
    /// Create a new listener for a specific channel.
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    /// * `channel` - The channel name to listen on
    ///
    /// # Returns
    /// A new PgNotifyListener instance
    ///
    /// # Errors
    /// Returns `sqlx::Error` if connection fails or LISTEN cannot be set up
    pub async fn new(pool: &sqlx::PgPool, channel: &str) -> Result<Self, sqlx::Error> {
        let mut listener = PgListener::connect_with(pool).await?;
        listener.listen(channel).await?;

        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

        // Clone for the background task
        let channel_name = channel.to_string();
        let mut listener_inner = listener.try_clone().expect("Failed to clone PgListener");

        // Spawn background task to forward notifications
        spawn(async move {
            loop {
                match listener_inner.recv().await {
                    Ok(notification) => {
                        trace!(
                            channel = notification.channel(),
                            payload = notification.payload(),
                            "Received PostgreSQL notification"
                        );
                        if tx.send(notification).await.is_err() {
                            trace!("Notification channel closed, stopping listener");
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "PgListener error, attempting reconnect...");
                        // Small delay before reconnect attempt
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                        // Try to re-establish connection
                        if let Err(reconnect_err) = listener_inner.listen(&channel_name).await {
                            error!(error = %reconnect_err, "Failed to reconnect to channel");
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            listener,
            tx,
            _rx: rx,
            channel: channel.to_string(),
        })
    }

    /// Create a listener for the commands outbox channel.
    ///
    /// This is a convenience method for creating a listener on the standard
    /// commands channel (`outbox_work`).
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    /// A new PgNotifyListener for commands
    pub async fn for_commands(pool: &sqlx::PgPool) -> Result<Self, sqlx::Error> {
        Self::new(pool, "outbox_work").await
    }

    /// Create a listener for the events outbox channel.
    ///
    /// This is a convenience method for creating a listener on the standard
    /// events channel (`event_work`).
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    /// A new PgNotifyListener for events
    pub async fn for_events(pool: &sqlx::PgPool) -> Result<Self, sqlx::Error> {
        Self::new(pool, "event_work").await
    }

    /// Send a notification to a channel.
    ///
    /// This can be used for testing or for manually triggering relay processing.
    ///
    /// # Arguments
    /// * `payload` - The payload to send
    ///
    /// # Returns
    /// Ok(()) if successful
    ///
    /// # Errors
    /// Returns `sqlx::Error` if the notification fails
    pub async fn notify(&self, payload: &str) -> Result<(), sqlx::Error> {
        self.listener.notify(&self.channel, Some(payload)).await
    }

    /// Get the channel name this listener is subscribed to.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Check if the listener connection is still valid.
    pub async fn is_valid(&self) -> bool {
        self.listener.try_ping().await.is_ok()
    }

    /// Get a receiver for consuming notifications.
    ///
    /// This allows the consumer to have more control over notification handling.
    pub fn receiver(&mut self) -> &mut mpsc::Receiver<PgNotification> {
        &mut self._rx
    }
}

/// Trait for types that can handle PostgreSQL notifications.
#[async_trait]
pub trait NotificationHandler: Send + Sync {
    /// Handle a notification.
    ///
    /// # Arguments
    /// * `notification` - The received notification
    async fn handle(&self, notification: PgNotification);
}

/// Null handler that discards all notifications (for testing).
#[derive(Debug, Default, Clone)]
pub struct NullNotificationHandler;

#[async_trait]
impl NotificationHandler for NullNotificationHandler {
    async fn handle(&self, _notification: PgNotification) {}
}

/// A simple notification handler that prints to logs.
#[derive(Debug, Default, Clone)]
pub struct LoggingNotificationHandler;

#[async_trait]
impl NotificationHandler for LoggingNotificationHandler {
    async fn handle(&self, notification: PgNotification) {
        debug!(
            channel = notification.channel(),
            payload = notification.payload(),
            "Notification received"
        );
    }
}

/// Builder for creating configured PgNotifyListener instances.
#[derive(Debug, Default)]
pub struct PgNotifyListenerBuilder {
    channel: String,
    capacity: usize,
}

impl PgNotifyListenerBuilder {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the channel name.
    pub fn channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = channel.into();
        self
    }

    /// Set the channel capacity.
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// Build the PgNotifyListener.
    pub async fn build(self, pool: &sqlx::PgPool) -> Result<PgNotifyListener, sqlx::Error> {
        let channel = if self.channel.is_empty() {
            "outbox_work"
        } else {
            &self.channel
        };

        let mut listener = PgListener::connect_with(pool).await?;
        listener.listen(channel).await?;

        let (tx, rx) = mpsc::channel(self.capacity);

        let channel_name = channel.to_string();
        let mut listener_inner = listener.try_clone().expect("Failed to clone PgListener");

        spawn(async move {
            loop {
                match listener_inner.recv().await {
                    Ok(notification) => {
                        if tx.send(notification).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "PgListener error");
                        break;
                    }
                }
            }
        });

        Ok(PgNotifyListener {
            listener,
            tx,
            _rx: rx,
            channel: channel_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    /// Create a test pool - requires DATABASE_URL env var or running PostgreSQL
    async fn create_test_pool() -> sqlx::PgPool {
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

        PgPoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .expect("Failed to create test pool")
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL")]
    async fn test_pg_notify_listener_creation() {
        let pool = create_test_pool().await;

        let listener = PgNotifyListener::for_commands(&pool).await.unwrap();

        assert_eq!(listener.channel(), "outbox_work");
        assert!(listener.is_valid().await);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL")]
    async fn test_pg_notify_listener_for_events() {
        let pool = create_test_pool().await;

        let listener = PgNotifyListener::for_events(&pool).await.unwrap();

        assert_eq!(listener.channel(), "event_work");
        assert!(listener.is_valid().await);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL")]
    async fn test_pg_notify_listener_notify() {
        let pool = create_test_pool().await;

        let mut listener = PgNotifyListener::for_commands(&pool).await.unwrap();

        // Send a notification
        let result = listener.notify("test-payload-123").await;
        assert!(result.is_ok());

        // Give some time for the notification to be received
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Try to receive (with timeout)
        let notification = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            listener.recv(),
        )
        .await
        .expect("Timeout waiting for notification")
        .expect("Failed to receive notification");

        assert_eq!(notification.channel(), "outbox_work");
        assert_eq!(notification.payload(), "test-payload-123");
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL")]
    async fn test_pg_notify_listener_builder() {
        let pool = create_test_pool().await;

        let listener = PgNotifyListenerBuilder::new()
            .channel("custom_channel")
            .capacity(50)
            .build(&pool)
            .await
            .unwrap();

        assert_eq!(listener.channel(), "custom_channel");
    }

    #[tokio::test]
    fn test_builder_defaults() {
        let builder = PgNotifyListenerBuilder::new();

        assert_eq!(builder.channel, "");
        assert_eq!(builder.capacity, 0);
    }

    #[tokio::test]
    fn test_notification_handler_traits() {
        // Verify that handlers implement the required traits
        let null_handler = NullNotificationHandler;
        let logging_handler = LoggingNotificationHandler;

        // These should compile if the traits are implemented correctly
        assert!(format!("{:?}", null_handler).len() > 0);
        assert!(format!("{:?}", logging_handler).len() > 0);
    }
}
