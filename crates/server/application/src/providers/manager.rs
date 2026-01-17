use crate::workers::WorkerProvisioningService;
use futures::StreamExt;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobQueue;
use hodei_server_domain::shared_kernel::{DomainError, Result};
use hodei_shared::event_topics::ALL_EVENTS;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

/// ProviderManager handles auto-scaling and worker provisioning based on events.
///
/// This is an abstract manager that delegates all provider-specific logic to
/// the `WorkerProvisioningService` trait. Concrete implementations (Docker,
/// Kubernetes, Cloud VMs, Firecracker, etc.) are handled by the provisioning service.
///
/// Listens to:
/// - `JobQueueDepthChanged`: Triggers worker provisioning when queue exceeds threshold
/// - `WorkerDisconnected`: Logs disconnection for health monitoring
/// - `ProviderHealthChanged`: Handles provider health state changes
/// - `WorkerTerminated`: Cleans up after worker termination
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
        let mut stream = self.event_bus.subscribe(ALL_EVENTS).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to subscribe to events: {}", e),
            }
        })?;

        info!("ProviderManager subscribed to events");

        let provisioning_service = self.provisioning_service.clone();
        let event_bus_publisher = self.event_bus.clone();
        let job_queue_publisher = self.job_queue.clone();

        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(event) => {
                        if let Err(e) = Self::handle_event(
                            &event,
                            &provisioning_service,
                            &event_bus_publisher,
                            &job_queue_publisher,
                        )
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
            warn!("ProviderManager event stream ended");
        });

        Ok(())
    }

    #[instrument(skip(provisioning_service, event_bus, job_queue))]
    async fn handle_event(
        event: &DomainEvent,
        provisioning_service: &Arc<P>,
        event_bus: &Arc<E>,
        job_queue: &Arc<Q>,
    ) -> Result<()> {
        match event {
            DomainEvent::JobQueueDepthChanged {
                queue_depth,
                threshold,
                correlation_id,
                ..
            } => {
                if queue_depth > threshold {
                    info!(
                        "📈 Queue depth {} exceeds threshold {}. Triggering auto-scaling.",
                        queue_depth, threshold
                    );

                    // Get available providers from the abstract provisioning service
                    let providers = provisioning_service
                        .list_providers()
                        .await
                        .unwrap_or_default();

                    if let Some(provider_id) = providers.first() {
                        // Check if provider is available
                        let is_available = provisioning_service
                            .is_provider_available(provider_id)
                            .await
                            .unwrap_or(false);

                        if !is_available {
                            warn!(
                                "⚠️ Provider {} is not available for provisioning",
                                provider_id
                            );
                            return Ok(());
                        }

                        let reason = format!("Queue depth {} > {}", queue_depth, threshold);

                        // Publish AutoScalingTriggered event
                        let trigger_event = DomainEvent::AutoScalingTriggered {
                            provider_id: provider_id.clone(),
                            reason: reason.clone(),
                            occurred_at: chrono::Utc::now(),
                            correlation_id: correlation_id.clone(),
                            actor: Some("ProviderManager".to_string()),
                        };

                        if let Err(e) = event_bus.publish(&trigger_event).await {
                            error!("Failed to publish AutoScalingTriggered: {}", e);
                        }

                        // Get the default worker spec from the provisioning service
                        // This is provider-agnostic - each provider defines its own defaults
                        let worker_spec = match provisioning_service
                            .default_worker_spec(provider_id)
                            .await
                        {
                            Some(spec) => spec,
                            None => {
                                warn!(
                                    "⚠️ No default worker spec for provider {}, cannot auto-provision",
                                    provider_id
                                );
                                return Ok(());
                            }
                        };

                        // Delegate provisioning to the abstract service
                        // The actual implementation (Docker, K8s, etc.) is hidden
                        info!(
                            "🚀 Requesting worker provisioning via provider {} (reason: {})",
                            provider_id, reason
                        );

                        // Get the next job_id from the queue for job-specific worker provisioning
                        let job_id = match job_queue.peek().await {
                            Ok(Some(job)) => job.id,
                            Ok(None) => {
                                warn!(
                                    "⚠️ Queue is empty, cannot provision worker for specific job"
                                );
                                return Ok(());
                            }
                            Err(e) => {
                                error!("⚠️ Failed to peek queue for job_id: {}", e);
                                return Ok(());
                            }
                        };

                        match provisioning_service
                            .provision_worker(provider_id, worker_spec, job_id)
                            .await
                        {
                            Ok(result) => {
                                info!(
                                    "✅ Worker {} provisioned successfully via provider {}",
                                    result.worker_id, result.provider_id
                                );
                            }
                            Err(e) => {
                                error!(
                                    "❌ Failed to provision worker via provider {}: {}",
                                    provider_id, e
                                );
                            }
                        }
                    } else {
                        warn!("⚠️ No providers available for auto-scaling");
                    }
                } else {
                    debug!(
                        "Queue depth {} within threshold {}, no scaling needed",
                        queue_depth, threshold
                    );
                }
            }

            DomainEvent::WorkerDisconnected { worker_id, .. } => {
                info!(
                    "🔌 Worker {} disconnected. May trigger replacement provisioning.",
                    worker_id
                );
                // Future: Could trigger replacement provisioning here based on policy
            }

            DomainEvent::WorkerTerminated {
                worker_id,
                provider_id,
                reason,
                ..
            } => {
                info!(
                    "💀 Worker {} terminated (provider: {}, reason: {})",
                    worker_id, provider_id, reason
                );
                // Future: Could update provider capacity tracking or trigger replacement
            }

            DomainEvent::ProviderHealthChanged {
                provider_id,
                old_status,
                new_status,
                ..
            } => {
                info!(
                    "🏥 Provider {} health changed: {:?} -> {:?}",
                    provider_id, old_status, new_status
                );
                // Future: Could failover to another provider if one becomes unhealthy
            }

            _ => {
                // Ignore other events
            }
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
