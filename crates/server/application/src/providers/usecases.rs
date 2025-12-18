use crate::ProviderRegistry;
use hodei_server_domain::providers::{ProviderConfig, ProviderTypeConfig};
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, Result};
use hodei_server_domain::workers::ProviderType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDto {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub status: String,
    pub priority: i32,
    pub max_workers: u32,
    pub active_workers: u32,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ProviderDto {
    fn from_domain(config: &ProviderConfig) -> Self {
        Self {
            id: config.id.to_string(),
            name: config.name.clone(),
            provider_type: config.provider_type.to_string(),
            status: config.status.to_string(),
            priority: config.priority,
            max_workers: config.max_workers,
            active_workers: config.active_workers,
            tags: config.tags.clone(),
            metadata: config.metadata.clone(),
            created_at: config.created_at.to_rfc3339(),
            updated_at: config.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterProviderRequest {
    pub name: String,
    pub provider_type: String,
    pub type_config: ProviderTypeConfig,
    pub priority: Option<i32>,
    pub max_workers: Option<u32>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl RegisterProviderRequest {
    pub fn builder(
        name: impl Into<String>,
        provider_type: impl Into<String>,
        type_config: ProviderTypeConfig,
    ) -> RegisterProviderRequestBuilder {
        RegisterProviderRequestBuilder {
            name: name.into(),
            provider_type: provider_type.into(),
            type_config,
            priority: None,
            max_workers: None,
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

pub struct RegisterProviderRequestBuilder {
    name: String,
    provider_type: String,
    type_config: ProviderTypeConfig,
    priority: Option<i32>,
    max_workers: Option<u32>,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
}

impl RegisterProviderRequestBuilder {
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    pub fn max_workers(mut self, max_workers: u32) -> Self {
        self.max_workers = Some(max_workers);
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> RegisterProviderRequest {
        RegisterProviderRequest {
            name: self.name,
            provider_type: self.provider_type,
            type_config: self.type_config,
            priority: self.priority,
            max_workers: self.max_workers,
            tags: self.tags,
            metadata: self.metadata,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterProviderResponse {
    pub provider: ProviderDto,
}

use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::request_context::RequestContext;

pub struct RegisterProviderUseCase {
    registry: Arc<ProviderRegistry>,
    event_bus: Arc<dyn EventBus>,
}

impl RegisterProviderUseCase {
    pub fn new(registry: Arc<ProviderRegistry>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            registry,
            event_bus,
        }
    }

    pub async fn execute(
        &self,
        request: RegisterProviderRequest,
    ) -> Result<RegisterProviderResponse> {
        self.execute_with_context(request, None).await
    }

    pub async fn execute_with_context(
        &self,
        request: RegisterProviderRequest,
        ctx: Option<&RequestContext>,
    ) -> Result<RegisterProviderResponse> {
        let provider_type = parse_provider_type(&request.provider_type)?;

        let mut config = self
            .registry
            .register_provider(request.name, provider_type, request.type_config)
            .await?;

        if let Some(priority) = request.priority {
            config.priority = priority;
        }
        if let Some(max_workers) = request.max_workers {
            config.max_workers = max_workers;
        }
        config.tags = request.tags;
        config.metadata = request.metadata;
        config.updated_at = Utc::now();

        self.registry.update_provider(config.clone()).await?;

        let correlation_id = ctx.map(|c| c.correlation_id().to_string());
        let actor = ctx.and_then(|c| c.actor_owned());

        // Publicar evento
        let event = DomainEvent::ProviderRegistered {
            provider_id: config.id.clone(),
            provider_type: config.provider_type.to_string(),
            config_summary: format!("Registered provider {}", config.name),
            occurred_at: Utc::now(),
            correlation_id,
            actor,
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!("Failed to publish ProviderRegistered event: {}", e);
        }

        Ok(RegisterProviderResponse {
            provider: ProviderDto::from_domain(&config),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProvidersRequest {
    pub only_enabled: bool,
    pub only_with_capacity: bool,
    pub provider_type: Option<String>,
}

impl Default for ListProvidersRequest {
    fn default() -> Self {
        Self {
            only_enabled: false,
            only_with_capacity: false,
            provider_type: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProvidersResponse {
    pub providers: Vec<ProviderDto>,
}

pub struct ListProvidersUseCase {
    registry: Arc<ProviderRegistry>,
}

impl ListProvidersUseCase {
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        Self { registry }
    }

    pub async fn execute(&self, request: ListProvidersRequest) -> Result<ListProvidersResponse> {
        let providers = if request.only_with_capacity {
            self.registry.list_providers_with_capacity().await?
        } else if request.only_enabled {
            self.registry.list_enabled_providers().await?
        } else if let Some(provider_type) = request.provider_type {
            let provider_type = parse_provider_type(&provider_type)?;
            self.registry.list_providers_by_type(&provider_type).await?
        } else {
            self.registry.list_providers().await?
        };

        Ok(ListProvidersResponse {
            providers: providers.iter().map(ProviderDto::from_domain).collect(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProviderResponse {
    pub provider: Option<ProviderDto>,
}

pub struct GetProviderUseCase {
    registry: Arc<ProviderRegistry>,
}

impl GetProviderUseCase {
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        Self { registry }
    }

    pub async fn execute(&self, provider_id: ProviderId) -> Result<GetProviderResponse> {
        let provider = self.registry.get_provider(&provider_id).await?;
        Ok(GetProviderResponse {
            provider: provider.as_ref().map(ProviderDto::from_domain),
        })
    }
}

/// DTO para Update Provider Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProviderRequest {
    pub provider_id: String,
    pub priority: Option<i32>,
    pub max_workers: Option<u32>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// DTO para Update Provider Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProviderResponse {
    pub provider: ProviderDto,
    pub changes: Vec<String>,
}

/// Use Case: Update Provider Configuration
///
/// Updates an existing provider's configuration and publishes a ProviderUpdated event.
pub struct UpdateProviderUseCase {
    registry: Arc<ProviderRegistry>,
    event_bus: Arc<dyn EventBus>,
}

impl UpdateProviderUseCase {
    pub fn new(registry: Arc<ProviderRegistry>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            registry,
            event_bus,
        }
    }

    pub async fn execute(&self, request: UpdateProviderRequest) -> Result<UpdateProviderResponse> {
        self.execute_with_context(request, None).await
    }

    pub async fn execute_with_context(
        &self,
        request: UpdateProviderRequest,
        ctx: Option<&RequestContext>,
    ) -> Result<UpdateProviderResponse> {
        let provider_id = uuid::Uuid::parse_str(&request.provider_id)
            .map(ProviderId::from_uuid)
            .map_err(|_| DomainError::InvalidProviderConfig {
                message: format!("Invalid provider_id: {}", request.provider_id),
            })?;

        let mut config = self
            .registry
            .get_provider(&provider_id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: provider_id.clone(),
            })?;

        let mut changes = Vec::new();

        // Apply updates and track changes
        if let Some(priority) = request.priority.filter(|&p| config.priority != p) {
            changes.push(format!("priority: {} -> {}", config.priority, priority));
            config.priority = priority;
        }

        if let Some(max_workers) = request.max_workers.filter(|&m| config.max_workers != m) {
            changes.push(format!(
                "max_workers: {} -> {}",
                config.max_workers, max_workers
            ));
            config.max_workers = max_workers;
        }

        if let Some(tags) = request.tags {
            if config.tags != tags {
                changes.push(format!("tags: {:?} -> {:?}", config.tags, tags));
                config.tags = tags;
            }
        }

        if let Some(metadata) = request.metadata {
            if config.metadata != metadata {
                changes.push("metadata: updated".to_string());
                config.metadata = metadata;
            }
        }

        // Only update if there are changes
        if changes.is_empty() {
            return Ok(UpdateProviderResponse {
                provider: ProviderDto::from_domain(&config),
                changes: vec![],
            });
        }

        config.updated_at = Utc::now();
        self.registry.update_provider(config.clone()).await?;

        let correlation_id = ctx.map(|c| c.correlation_id().to_string());
        let actor = ctx.and_then(|c| c.actor_owned());

        // Publish ProviderUpdated event
        let event = DomainEvent::ProviderUpdated {
            provider_id: config.id.clone(),
            changes: Some(changes.join(", ")),
            occurred_at: Utc::now(),
            correlation_id,
            actor,
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!("Failed to publish ProviderUpdated event: {}", e);
        }

        Ok(UpdateProviderResponse {
            provider: ProviderDto::from_domain(&config),
            changes,
        })
    }
}

fn parse_provider_type(s: &str) -> Result<ProviderType> {
    if s.trim().is_empty() {
        return Err(DomainError::InvalidProviderConfig {
            message: "provider_type is required".to_string(),
        });
    }

    match s.to_lowercase().as_str() {
        "docker" => Ok(ProviderType::Docker),
        "kubernetes" | "k8s" => Ok(ProviderType::Kubernetes),
        "fargate" => Ok(ProviderType::Fargate),
        "cloud_run" | "cloudrun" => Ok(ProviderType::CloudRun),
        "container_apps" | "containerapps" => Ok(ProviderType::ContainerApps),
        "lambda" => Ok(ProviderType::Lambda),
        "cloud_functions" | "cloudfunctions" => Ok(ProviderType::CloudFunctions),
        "azure_functions" | "azurefunctions" => Ok(ProviderType::AzureFunctions),
        "ec2" => Ok(ProviderType::EC2),
        "compute_engine" | "computeengine" => Ok(ProviderType::ComputeEngine),
        "azure_vms" | "azurevms" => Ok(ProviderType::AzureVMs),
        "bare_metal" | "baremetal" => Ok(ProviderType::BareMetal),
        other if other.starts_with("custom:") => Ok(ProviderType::Custom(
            other
                .strip_prefix("custom:")
                .unwrap_or("unknown")
                .to_string(),
        )),
        other => Ok(ProviderType::Custom(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::stream::BoxStream;
    use hodei_server_domain::event_bus::EventBusError;
    use hodei_server_domain::providers::{DockerConfig, ProviderConfig, ProviderConfigRepository};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::sync::RwLock;

    struct MockEventBus {
        published: Arc<Mutex<Vec<DomainEvent>>>,
    }

    impl MockEventBus {
        fn new() -> Self {
            Self {
                published: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl EventBus for MockEventBus {
        async fn publish(&self, event: &DomainEvent) -> std::result::Result<(), EventBusError> {
            self.published.lock().unwrap().push(event.clone());
            Ok(())
        }
        async fn subscribe(
            &self,
            _topic: &str,
        ) -> std::result::Result<
            BoxStream<'static, std::result::Result<DomainEvent, EventBusError>>,
            EventBusError,
        > {
            Err(EventBusError::SubscribeError("Mock".to_string()))
        }
    }

    struct MockProviderConfigRepository {
        configs: RwLock<HashMap<ProviderId, ProviderConfig>>,
    }

    impl MockProviderConfigRepository {
        fn new() -> Self {
            Self {
                configs: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl hodei_server_domain::providers::ProviderConfigRepository for MockProviderConfigRepository {
        async fn save(&self, config: &ProviderConfig) -> Result<()> {
            self.configs
                .write()
                .await
                .insert(config.id.clone(), config.clone());
            Ok(())
        }
        async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>> {
            Ok(self.configs.read().await.get(id).cloned())
        }
        async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
            Ok(self
                .configs
                .read()
                .await
                .values()
                .find(|c| c.name == name)
                .cloned())
        }
        async fn find_by_type(&self, _pt: &ProviderType) -> Result<Vec<ProviderConfig>> {
            Ok(vec![])
        }
        async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
            Ok(vec![])
        }
        async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
            Ok(vec![])
        }
        async fn find_all(&self) -> Result<Vec<ProviderConfig>> {
            Ok(self.configs.read().await.values().cloned().collect())
        }
        async fn update(&self, config: &ProviderConfig) -> Result<()> {
            self.configs
                .write()
                .await
                .insert(config.id.clone(), config.clone());
            Ok(())
        }
        async fn delete(&self, id: &ProviderId) -> Result<()> {
            self.configs.write().await.remove(id);
            Ok(())
        }
        async fn exists_by_name(&self, name: &str) -> Result<bool> {
            Ok(self.configs.read().await.values().any(|c| c.name == name))
        }
    }

    fn create_docker_config() -> hodei_server_domain::providers::ProviderTypeConfig {
        hodei_server_domain::providers::ProviderTypeConfig::Docker(DockerConfig::default())
    }

    #[tokio::test]
    async fn test_update_provider_publishes_event() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let event_bus = Arc::new(MockEventBus::new());
        let registry = Arc::new(ProviderRegistry::new(repo.clone()));

        // First register a provider
        let config = ProviderConfig::new(
            "test-provider".to_string(),
            ProviderType::Docker,
            create_docker_config(),
        );
        repo.save(&config).await.unwrap();

        let use_case = UpdateProviderUseCase::new(registry, event_bus.clone());

        let request = UpdateProviderRequest {
            provider_id: config.id.to_string(),
            priority: Some(100),
            max_workers: Some(20),
            tags: None,
            metadata: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.provider.priority, 100);
        assert_eq!(response.provider.max_workers, 20);
        assert_eq!(response.changes.len(), 2);

        let events = event_bus.published.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::ProviderUpdated {
                provider_id,
                changes,
                ..
            } => {
                assert_eq!(provider_id, &config.id);
                assert!(changes.is_some());
            }
            _ => panic!("Expected ProviderUpdated event"),
        }
    }

    #[tokio::test]
    async fn test_update_provider_no_changes_no_event() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let event_bus = Arc::new(MockEventBus::new());
        let registry = Arc::new(ProviderRegistry::new(repo.clone()));

        let config = ProviderConfig::new(
            "test-provider".to_string(),
            ProviderType::Docker,
            create_docker_config(),
        );
        repo.save(&config).await.unwrap();

        let use_case = UpdateProviderUseCase::new(registry, event_bus.clone());

        // Request with same values (no actual changes)
        let request = UpdateProviderRequest {
            provider_id: config.id.to_string(),
            priority: Some(config.priority),
            max_workers: Some(config.max_workers),
            tags: None,
            metadata: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.changes.is_empty());

        // No event should be published
        let events = event_bus.published.lock().unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_update_provider_not_found() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let event_bus = Arc::new(MockEventBus::new());
        let registry = Arc::new(ProviderRegistry::new(repo));

        let use_case = UpdateProviderUseCase::new(registry, event_bus);

        let request = UpdateProviderRequest {
            provider_id: uuid::Uuid::new_v4().to_string(),
            priority: Some(100),
            max_workers: None,
            tags: None,
            metadata: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }
}
