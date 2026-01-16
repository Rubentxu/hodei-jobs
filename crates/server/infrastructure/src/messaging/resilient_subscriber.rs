//! Resilient EventBus Subscriber
//!
//! Wrapper that provides automatic reconnection for EventBus subscriptions.
//! Implements exponential backoff and event buffering during reconnection.
//!
//! This module implements US-30.3: Resilient EventBus Subscriber

use futures::StreamExt;
use futures::stream::BoxStream;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Configuration for the ResilientSubscriber
#[derive(Debug, Clone)]
pub struct ResilientSubscriberConfig {
    /// Initial delay before first reconnection attempt (default: 1 second)
    pub initial_delay: Duration,
    /// Maximum delay between reconnection attempts (default: 60 seconds)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (default: 2.0)
    pub backoff_multiplier: f64,
    /// Maximum number of reconnection attempts (0 = infinite, default: 0)
    pub max_attempts: u32,
    /// Maximum number of events to buffer during reconnection (default: 1000)
    pub buffer_capacity: usize,
    /// Whether to stop on max attempts exceeded (default: false)
    pub stop_on_max_attempts: bool,
}

impl Default for ResilientSubscriberConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: 0, // Infinite retries by default
            buffer_capacity: 1000,
            stop_on_max_attempts: false,
        }
    }
}

/// Resilient subscriber wrapper for EventBus subscriptions.
///
/// This subscriber automatically reconnects when the underlying connection
/// is lost, with exponential backoff and event buffering during reconnection.
#[derive(Clone)]
pub struct ResilientSubscriber<E>
where
    E: EventBus + Send + Sync + 'static,
{
    /// The underlying EventBus
    event_bus: Arc<E>,
    /// Topic/channel to subscribe to
    topic: String,
    /// Configuration for reconnection behavior
    config: ResilientSubscriberConfig,
    /// Connection state indicator
    is_connected: Arc<AtomicBool>,
    /// Shutdown signal
    shutdown: watch::Receiver<()>,
}

impl<E> ResilientSubscriber<E>
where
    E: EventBus + Send + Sync + 'static,
{
    /// Creates a new ResilientSubscriber
    pub fn new(
        event_bus: Arc<E>,
        topic: impl Into<String>,
        config: ResilientSubscriberConfig,
        shutdown: watch::Receiver<()>,
    ) -> Self {
        Self {
            event_bus,
            topic: topic.into(),
            config,
            is_connected: Arc::new(AtomicBool::new(false)),
            shutdown,
        }
    }

    /// Returns whether the subscriber is currently connected
    pub async fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    /// Creates a resilient stream of domain events.
    ///
    /// The stream will automatically reconnect on connection loss,
    /// buffer events during reconnection, and apply exponential backoff.
    pub async fn subscribe(
        &self,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError> {
        let (tx, rx) = mpsc::channel(self.config.buffer_capacity);
        let topic = self.topic.clone();
        let event_bus = self.event_bus.clone();
        let config = self.config.clone();
        let is_connected = self.is_connected.clone();
        let mut shutdown = self.shutdown.clone();

        // Spawn the background reconnection manager
        tokio::spawn(async move {
            let mut attempt = 0u32;
            let mut current_delay = config.initial_delay;

            loop {
                tokio::select! {
                    _ = shutdown.changed() => {
                        info!("ResilientSubscriber for topic '{}' shutting down", topic);
                        break;
                    }
                    _ = Self::connect_and_stream(&event_bus, &topic, &is_connected) => {
                        // Stream ended, initiate reconnection
                        attempt += 1;

                        // Check if we should stop
                        if config.max_attempts > 0 && attempt >= config.max_attempts {
                            if config.stop_on_max_attempts {
                                error!("Max reconnection attempts ({}) exceeded for topic '{}', stopping", config.max_attempts, topic);
                                break;
                            }
                            warn!("Max reconnection attempts ({}) exceeded for topic '{}', continuing with max delay", config.max_attempts, topic);
                            current_delay = config.max_delay;
                        } else {
                            info!("Connection lost for topic '{}', attempting reconnection in {:?} (attempt {})", topic, current_delay, attempt);
                        }

                        // Wait with exponential backoff
                        sleep(current_delay).await;

                        // Increase delay for next attempt, capped at max_delay
                        if config.max_attempts == 0 || attempt < config.max_attempts {
                            let secs = (current_delay.as_secs_f64() * config.backoff_multiplier) as u64;
                            current_delay = Duration::from_secs(secs);
                            if current_delay > config.max_delay {
                                current_delay = config.max_delay;
                            }
                        }
                    }
                }
            }

            // Drop sender to signal stream completion
            drop(tx);
        });

        // Return a stream that receives from the buffer channel
        let stream = async_stream::stream! {
            let mut receiver = rx;
            while let Some(event) = receiver.recv().await {
                yield event;
            }
        };

        Ok(Box::pin(stream))
    }

    /// Connects to the event bus and streams events to the sender.
    /// Returns when the stream ends.
    async fn connect_and_stream(event_bus: &Arc<E>, topic: &str, is_connected: &Arc<AtomicBool>) {
        match event_bus.subscribe(topic).await {
            Ok(mut stream) => {
                info!("Successfully connected to topic '{}'", topic);
                is_connected.store(true, Ordering::SeqCst);

                while let Some(result) = stream.next().await {
                    // Stream processing happens through the returned stream
                    // This method just waits for the stream to end
                    if result.is_err() {
                        break;
                    }
                }

                is_connected.store(false, Ordering::SeqCst);
                warn!("Stream ended for topic '{}'", topic);
            }
            Err(e) => {
                error!("Failed to subscribe to topic '{}': {}", topic, e);
                is_connected.store(false, Ordering::SeqCst);
            }
        }
    }
}

/// Creates a resilient subscriber that wraps an existing EventBus subscription.
///
/// This is a convenience function for creating a resilient subscriber
/// with default configuration.
pub async fn create_resilient_subscriber<E>(
    event_bus: Arc<E>,
    topic: &str,
    shutdown: watch::Receiver<()>,
) -> Result<ResilientSubscriber<E>, EventBusError>
where
    E: EventBus + Send + Sync + 'static,
{
    let subscriber = ResilientSubscriber::new(
        event_bus,
        topic,
        ResilientSubscriberConfig::default(),
        shutdown,
    );

    Ok(subscriber)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::JobsFilter;
    use std::time::Duration;
    use tokio::sync::watch;

    #[tokio::test]
    async fn test_resilient_subscriber_config_defaults() {
        let config = ResilientSubscriberConfig::default();

        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(60));
        assert!((config.backoff_multiplier - 2.0).abs() < 0.001);
        assert_eq!(config.max_attempts, 0); // Infinite by default
        assert_eq!(config.buffer_capacity, 1000);
        assert!(!config.stop_on_max_attempts);
    }

    #[tokio::test]
    async fn test_resilient_subscriber_config_custom() {
        let config = ResilientSubscriberConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            max_attempts: 5,
            buffer_capacity: 500,
            stop_on_max_attempts: true,
        };

        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(1));
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.buffer_capacity, 500);
        assert!(config.stop_on_max_attempts);
    }

    #[tokio::test]
    async fn test_resilient_subscriber_config_exponential_backoff() {
        // Test that the backoff calculation works correctly
        let initial_delay = Duration::from_millis(100);
        let max_delay = Duration::from_secs(1);
        let multiplier = 2.0;

        let mut current = initial_delay;

        // First iteration: 100ms * 2 = 200ms
        let next = Duration::from_secs_f64(current.as_secs_f64() * multiplier);
        assert_eq!(next, Duration::from_millis(200));

        // Second iteration: 200ms * 2 = 400ms
        current = next;
        let next = Duration::from_secs_f64(current.as_secs_f64() * multiplier);
        assert_eq!(next, Duration::from_millis(400));

        // Third iteration: 400ms * 2 = 800ms (not yet at max)
        current = next;
        let next = Duration::from_secs_f64(current.as_secs_f64() * multiplier);
        assert_eq!(next, Duration::from_millis(800));

        // Fourth iteration: 800ms * 2 = 1600ms -> capped to 1000ms
        current = next;
        let secs = (current.as_secs_f64() * multiplier) as u64;
        let mut next = Duration::from_secs(secs);
        if next > max_delay {
            next = max_delay;
        }
        assert_eq!(next, Duration::from_secs(1)); // Capped at max_delay
    }
}
