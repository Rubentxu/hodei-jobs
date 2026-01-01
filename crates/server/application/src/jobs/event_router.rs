//! Event Router - Pure Routing Without Orchestration
//!
//! EPIC-33: Simplifies JobCoordinator by extracting pure event routing.

use futures::StreamExt;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{error, info, warn};

/// Event Router - Pure Delegation
pub struct EventRouter {
    event_bus: Arc<dyn EventBus>,
    topics: Vec<String>,
    running: Arc<AtomicBool>,
}

impl EventRouter {
    pub fn new(event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            event_bus,
            topics: Vec::new(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn subscribe(mut self, topic: &str) -> Self {
        self.topics.push(topic.to_string());
        self
    }

    pub async fn start(&self) -> Result<(), EventBusError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        for topic in &self.topics {
            let topic = topic.clone();
            let event_bus: Arc<dyn EventBus> = self.event_bus.clone();
            let running = self.running.clone();
            info!(%topic, "EventRouter: Subscribing to topic");
            match event_bus.subscribe(&topic).await {
                Ok(stream) => {
                    tokio::spawn(async move {
                        let mut stream = stream;
                        while let Some(result) = stream.next().await {
                            match result {
                                Ok(_) => {}
                                Err(e) => error!(%topic, "Event stream error: {}", e),
                            }
                        }
                        running.store(false, Ordering::SeqCst);
                        warn!(%topic, "Event stream ended for topic");
                    });
                }
                Err(e) => {
                    self.running.store(false, Ordering::SeqCst);
                    error!(%topic, "Failed to subscribe: {}", e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Simplified coordinator
pub struct SimpleJobCoordinator {
    event_router: EventRouter,
}

impl SimpleJobCoordinator {
    pub fn new(event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            event_router: EventRouter::new(event_bus),
        }
    }

    pub fn with_topic(mut self, topic: &str) -> Self {
        self.event_router = self.event_router.subscribe(topic);
        self
    }

    pub async fn start(&self) -> Result<(), EventBusError> {
        self.event_router.start().await
    }
}
