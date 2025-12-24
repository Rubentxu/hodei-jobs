//! Event Subscriber Component
//!
//! Responsible for subscribing to domain events and triggering job scheduling.
//! Follows Single Responsibility Principle: only handles event subscription.

use async_trait::async_trait;
use futures::StreamExt;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Event handler trait for processing domain events
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> anyhow::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Event Subscriber
///
/// Subscribes to domain events from the event bus and delegates processing
/// to registered event handlers. This component has a single responsibility:
/// manage event subscription and routing.
pub struct EventSubscriber {
    event_bus: Arc<dyn EventBus>,
    handlers: Vec<Arc<dyn EventHandler>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl EventSubscriber {
    /// Create a new EventSubscriber
    pub fn new(event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            event_bus,
            handlers: Vec::new(),
            shutdown_tx: None,
        }
    }

    /// Add an event handler
    /// Builder Pattern: returns self for fluent configuration
    pub fn with_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.handlers.push(handler);
        self
    }

    /// Subscribe to events and start processing
    /// Returns a shutdown signal receiver
    pub async fn subscribe(
        &mut self,
    ) -> anyhow::Result<mpsc::Receiver<()>, Box<dyn std::error::Error + Send + Sync>> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Subscribe to the event topic
        let mut stream = self
            .event_bus
            .subscribe("hodei_events")
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Spawn event processing loop
        let handlers = self.handlers.clone();
        let shutdown_tx = self.shutdown_tx.clone().unwrap();

        tokio::spawn(async move {
            info!("📡 EventSubscriber: Starting event processing loop");

            loop {
                tokio::select! {
                    // Process events
                    result = stream.next() => {
                        match result {
                            Some(Ok(event)) => {
                                debug!("📨 EventSubscriber: Received event {:?}", event);

                                // Route event to all registered handlers
                                for handler in &handlers {
                                    if let Err(e) = handler.handle_event(event.clone()).await {
                                        error!("❌ EventSubscriber: Handler error: {}", e);
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                error!("❌ EventSubscriber: Error receiving event: {}", e);
                            }
                            None => {
                                warn!("⚠️ EventSubscriber: Event stream ended unexpectedly");
                                break;
                            }
                        }
                    }
                    // Check for shutdown signal
                    _ = shutdown_tx.closed() => {
                        info!("🛑 EventSubscriber: Received shutdown signal");
                        break;
                    }
                }
            }

            info!("📡 EventSubscriber: Event processing loop ended");
        });

        Ok(shutdown_rx)
    }

    /// Gracefully shutdown the event subscriber
    pub async fn shutdown(self) -> anyhow::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(tx) = self.shutdown_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }
}
