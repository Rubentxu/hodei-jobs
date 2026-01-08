//! PgNotifyListener - PostgreSQL LISTEN/NOTIFY wrapper
//!
//! Provides reactive notifications for outbox processing using PostgreSQL's
//! LISTEN/NOTIFY mechanism.

use sqlx::postgres::{PgListener, PgNotification};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, trace};

/// Wrapper for PostgreSQL LISTEN/NOTIFY mechanism.
#[derive(Debug)]
pub struct PgNotifyListener {
    listener: PgListener,
    channel: String,
}

impl PgNotifyListener {
    /// Create a new listener for a specific channel.
    pub async fn new(pool: &sqlx::PgPool, channel: &str) -> Result<Self, sqlx::Error> {
        let mut listener = PgListener::connect_with(pool).await?;
        listener.listen(channel).await?;

        Ok(Self {
            listener,
            channel: channel.to_string(),
        })
    }

    /// Create a listener for the commands outbox channel.
    pub async fn for_commands(pool: &sqlx::PgPool) -> Result<Self, sqlx::Error> {
        Self::new(pool, "outbox_work").await
    }

    /// Create a listener for the events outbox channel.
    pub async fn for_events(pool: &sqlx::PgPool) -> Result<Self, sqlx::Error> {
        Self::new(pool, "event_work").await
    }

    /// Get the channel name.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Receive the next notification (blocking).
    pub async fn recv(&mut self) -> Result<PgNotification, sqlx::Error> {
        self.listener.recv().await
    }
}

/// Builder for PgNotifyListener.
#[derive(Debug, Default)]
pub struct PgNotifyListenerBuilder {
    channel: String,
}

impl PgNotifyListenerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = channel.into();
        self
    }

    pub async fn build(self, pool: &sqlx::PgPool) -> Result<PgNotifyListener, sqlx::Error> {
        let channel = if self.channel.is_empty() {
            "outbox_work"
        } else {
            &self.channel
        };
        PgNotifyListener::new(pool, channel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = PgNotifyListenerBuilder::new();
        assert!(builder.channel.is_empty());
    }

    #[test]
    fn test_builder_channel() {
        let builder = PgNotifyListenerBuilder::new()
            .channel("custom_channel");
        assert_eq!(builder.channel, "custom_channel");
    }
}
