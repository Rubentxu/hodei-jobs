//! SignalDispatcher port for NATS Core Pub/Sub.
//!
//! This module defines the [`SignalDispatcher`] trait for notifying
//! workers about saga events. It uses lightweight Pub/Sub for efficiency.

use super::super::event::SagaId;
use futures::stream::Stream;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Errors from signal dispatch operations.
#[derive(Debug, thiserror::Error)]
pub enum SignalDispatcherError<E> {
    #[error("Signal publish failed: {0}")]
    Publish(E),

    #[error("Signal subscription failed: {0}")]
    Subscribe(E),

    #[error("Subscription stream ended unexpectedly")]
    StreamEnded,
}

/// A subscription to saga signals.
///
/// Created by [`SignalDispatcher::subscribe`], this stream yields
/// signal notifications as they occur.
#[derive(Debug)]
pub struct SignalSubscription {
    /// The saga ID this subscription is for.
    pub saga_id: SagaId,
    // The actual receiver would be implementation-specific
}

impl SignalSubscription {
    /// Create a new signal subscription.
    pub fn new(saga_id: SagaId) -> Self {
        Self { saga_id }
    }
}

/// Stream of signal notifications.
pub struct SignalStream {
    // Implementation-specific receiver
    receiver: tokio::sync::mpsc::Receiver<SignalNotification>,
}

impl SignalStream {
    /// Create a new signal stream.
    pub fn new(receiver: tokio::sync::mpsc::Receiver<SignalNotification>) -> Self {
        Self { receiver }
    }
}

impl Stream for SignalStream {
    type Item = SignalNotification;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// A notification that a signal has been sent.
#[derive(Debug, Clone)]
pub struct SignalNotification {
    /// The saga ID this signal is for.
    pub saga_id: SagaId,
    /// The type of signal.
    pub signal_type: SignalType,
    /// Additional payload for the signal.
    pub payload: Vec<u8>,
}

/// Types of signals that can be dispatched.
#[derive(Debug, Clone)]
pub enum SignalType {
    /// A new event has been appended to the saga.
    NewEvent,

    /// A timer has fired.
    TimerFired,

    /// The saga has been cancelled.
    Cancelled,

    /// External signal received.
    External(String),
}

/// Trait for dispatching signals to workers.
///
/// The SignalDispatcher is responsible for notifying workers about
/// saga events using lightweight Pub/Sub messaging. This allows workers
/// to react immediately to new events rather than polling.
#[async_trait::async_trait]
pub trait SignalDispatcher: Send + Sync {
    /// The error type for this implementation.
    type Error: Debug + Send + Sync + 'static;

    /// Notify that a new event has been appended to a saga.
    ///
    /// This is called after `EventStore::append_event` succeeds.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga that received a new event.
    /// * `event_id` - The ID of the new event.
    async fn notify_new_event(
        &self,
        saga_id: &SagaId,
        event_id: u64,
    ) -> Result<(), SignalDispatcherError<Self::Error>>;

    /// Notify that a timer has fired.
    ///
    /// This is called when a durable timer expires.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga the timer belongs to.
    /// * `timer_id` - The ID of the fired timer.
    async fn notify_timer_fired(
        &self,
        saga_id: &SagaId,
        timer_id: &str,
    ) -> Result<(), SignalDispatcherError<Self::Error>>;

    /// Notify that a saga has been cancelled.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The cancelled saga.
    async fn notify_cancelled(
        &self,
        saga_id: &SagaId,
    ) -> Result<(), SignalDispatcherError<Self::Error>>;

    /// Subscribe to signals for a specific saga pattern.
    ///
    /// The pattern may include wildcards for matching multiple sagas.
    /// Common patterns:
    /// - `"saga.<saga_id>"` - Specific saga
    /// - `"saga.*"` - All sagas
    /// - `"timer.*"` - All timer signals
    ///
    /// # Arguments
    ///
    /// * `pattern` - The subscription pattern.
    ///
    /// # Returns
    ///
    /// A stream of signal notifications.
    async fn subscribe(
        &self,
        pattern: &str,
    ) -> Result<SignalStream, SignalDispatcherError<Self::Error>>;

    /// Publish an external signal to a saga.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The target saga.
    /// * `signal_name` - Name of the signal.
    /// * `payload` - Signal payload.
    async fn send_signal(
        &self,
        saga_id: &SagaId,
        signal_name: &str,
        payload: &[u8],
    ) -> Result<(), SignalDispatcherError<Self::Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::SagaId;

    #[tokio::test]
    async fn test_signal_notification() {
        let notification = SignalNotification {
            saga_id: SagaId("test-saga".to_string()),
            signal_type: SignalType::NewEvent,
            payload: vec![1, 2, 3],
        };

        assert_eq!(notification.saga_id.0, "test-saga");
        // Just check we can construct the type
        matches!(notification.signal_type, SignalType::NewEvent);
    }
}
