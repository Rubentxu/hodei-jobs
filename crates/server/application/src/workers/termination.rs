//! Worker Termination Service
use chrono::Utc;
use hodei_server_domain::events::{DomainEvent, TerminationReason};
use hodei_server_domain::outbox::OutboxEventInsert;
use hodei_server_domain::shared_kernel::{ProviderId, Result, WorkerId, WorkerState};
use std::sync::Arc;
use std::time::Duration;

pub struct WorkerTerminationConfig {
    pub max_destruction_retries: u32,
    pub retry_base_delay: Duration,
}

impl Default for WorkerTerminationConfig {
    fn default() -> Self {
        Self {
            max_destruction_retries: 3,
            retry_base_delay: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminationResult {
    pub worker_id: WorkerId,
    pub success: bool,
    pub provider_id: ProviderId,
}

#[async_trait::async_trait]
pub trait ProviderOperationsPort: Send + Sync {
    async fn get_provider(&self, provider_id: &ProviderId) -> Result<Option<ProviderHandle>>;
    async fn destroy_worker(&self, provider: &ProviderHandle, handle: &WorkerHandle) -> Result<()>;
}

pub struct ProviderHandle {
    pub provider_id: ProviderId,
}

pub struct WorkerHandle {
    pub worker_id: WorkerId,
    pub provider_resource_id: String,
}

#[async_trait::async_trait]
pub trait TerminationEventPort: Send + Sync {
    async fn publish_event(&self, event: &DomainEvent) -> Result<()>;
    async fn insert_outbox_events(&self, events: &[OutboxEventInsert]) -> Result<()>;
}

#[async_trait::async_trait]
pub trait WorkerRegistryPort: Send + Sync {
    async fn get_worker(&self, worker_id: &WorkerId) -> Result<Option<WorkerInfo>>;
    async fn update_state(&self, worker_id: &WorkerId, state: WorkerState) -> Result<()>;
    async fn unregister(&self, worker_id: &WorkerId) -> Result<()>;
}

pub struct WorkerInfo {
    pub id: WorkerId,
    pub state: WorkerState,
    pub provider_id: ProviderId,
}

pub struct WorkerTerminationService {
    registry_port: Arc<dyn WorkerRegistryPort>,
    provider_port: Arc<dyn ProviderOperationsPort>,
    event_port: Arc<dyn TerminationEventPort>,
    config: WorkerTerminationConfig,
}

impl WorkerTerminationService {
    pub fn new(
        registry_port: Arc<dyn WorkerRegistryPort>,
        provider_port: Arc<dyn ProviderOperationsPort>,
        event_port: Arc<dyn TerminationEventPort>,
    ) -> Self {
        Self {
            registry_port,
            provider_port,
            event_port,
            config: WorkerTerminationConfig::default(),
        }
    }

    pub async fn terminate_ephemeral_worker(
        &self,
        worker_id: &WorkerId,
    ) -> Result<TerminationResult> {
        let worker = self
            .registry_port
            .get_worker(worker_id)
            .await?
            .ok_or_else(
                || hodei_server_domain::shared_kernel::DomainError::WorkerNotFound {
                    worker_id: worker_id.clone(),
                },
            )?;

        self.registry_port
            .update_state(worker_id, WorkerState::Terminated)
            .await?;

        let event = DomainEvent::WorkerEphemeralTerminating {
            worker_id: worker.id.clone(),
            provider_id: worker.provider_id.clone(),
            reason: TerminationReason::Unregistered,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: Some("WorkerTerminationService".to_string()),
        };
        self.event_port.publish_event(&event).await?;

        self.registry_port.unregister(worker_id).await?;

        Ok(TerminationResult {
            worker_id: worker_id.clone(),
            success: true,
            provider_id: worker.provider_id,
        })
    }
}
