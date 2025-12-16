use crate::ProviderRegistry;
use hodei_jobs_domain::provider_config::{ProviderConfig, ProviderTypeConfig};
use hodei_jobs_domain::shared_kernel::{DomainError, ProviderId, Result};
use hodei_jobs_domain::worker::ProviderType;
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
use hodei_jobs_domain::event_bus::EventBus;
use hodei_jobs_domain::events::DomainEvent;
use hodei_jobs_domain::request_context::RequestContext;

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
