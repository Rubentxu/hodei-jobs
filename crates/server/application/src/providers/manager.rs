use crate::workers::WorkerProvisioningService;
use futures::StreamExt;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobQueue;
use hodei_server_domain::shared_kernel::{ProviderId, Result};
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

pub struct ProviderManager<E, P, Q>
where
    E: EventBus + 'static,
    P: WorkerProvisioningService + 'static,
    Q: JobQueue + 'static,
{
    event_bus: Arc<E>,
    provisioning_service: Arc<P>,
    job_queue: Arc<Q>,
}

impl<E, P, Q> ProviderManager<E, P, Q>
where
    E: EventBus,
    P: WorkerProvisioningService,
    Q: JobQueue,
{
    pub fn new(event_bus: Arc<E>, provisioning_service: Arc<P>, job_queue: Arc<Q>) -> Self {
        Self {
            event_bus,
            provisioning_service,
            job_queue,
        }
    }

    pub async fn subscribe_to_events(&self) -> Result<()> {
        let event_bus = self.event_bus.clone();
        let manager = Arc::new(self.clone()); // Need to be careful with cloning self, likely need Arc wrapper around inner logic or structure

        // Since we can't easily clone 'self' into the async block if it holds Arcs directly without being Arc itself or having internal Arcs.
        // Let's assume we spawn a task that captures Arcs.

        let mut stream = event_bus.subscribe("hodei_events").await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to subscribe to events: {}", e),
            }
        })?;

        info!("ProviderManager subscribed to events");

        let provisioning_service = self.provisioning_service.clone();
        let event_bus_publisher = self.event_bus.clone();

        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(event) => {
                        if let Err(e) =
                            Self::handle_event(&event, &provisioning_service, &event_bus_publisher)
                                .await
                        {
                            error!("Error handling event: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving event: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    #[instrument(skip(provisioning_service, event_bus))]
    async fn handle_event(
        event: &DomainEvent,
        provisioning_service: &Arc<P>,
        event_bus: &Arc<E>,
    ) -> Result<()> {
        match event {
            DomainEvent::JobQueueDepthChanged {
                queue_depth,
                threshold,
                ..
            } => {
                if queue_depth > threshold {
                    info!(
                        "Queue depth {} exceeds threshold {}. Triggering auto-scaling.",
                        queue_depth, threshold
                    );
                    // logic to trigger scaling
                    // For now, simpler logic: just provision 1 worker using default provider

                    // 1. Get available providers
                    let providers = provisioning_service
                        .list_providers()
                        .await
                        .unwrap_or_default();
                    if let Some(provider_id) = providers.first() {
                        info!("Auto-scaling using provider {}", provider_id);
                        let reason = format!("Queue depth {} > {}", queue_depth, threshold);

                        // Publish AutoScalingTriggered
                        let trigger_event = DomainEvent::AutoScalingTriggered {
                            provider_id: provider_id.clone(),
                            reason,
                            occurred_at: chrono::Utc::now(),
                            correlation_id: None,
                            actor: Some("ProviderManager".to_string()),
                        };

                        if let Err(e) = event_bus.publish(&trigger_event).await {
                            error!("Failed to publish AutoScalingTriggered: {}", e);
                        }

                        // In a real scenario, we would call provision_worker here async or delegate to another service
                        // provisioning_service.provision_worker(...)
                    } else {
                        warn!("No providers available for auto-scaling");
                    }
                }
            }
            DomainEvent::WorkerDisconnected { worker_id, .. } => {
                info!(
                    "Worker {} disconnected. Checking provider health.",
                    worker_id
                );
                // Implementation for health check logic
            }
            _ => {}
        }
        Ok(())
    }
}

// Clone implementation to allow moving into spawn
impl<E, P, Q> Clone for ProviderManager<E, P, Q>
where
    E: EventBus,
    P: WorkerProvisioningService,
    Q: JobQueue,
{
    fn clone(&self) -> Self {
        Self {
            event_bus: self.event_bus.clone(),
            provisioning_service: self.provisioning_service.clone(),
            job_queue: self.job_queue.clone(),
        }
    }
}
