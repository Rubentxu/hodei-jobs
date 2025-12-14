// Provider Management gRPC Service Implementation

use hodei_jobs::providers::{
    provider_management_service_server::ProviderManagementService,
    DeleteProviderRequest, DeleteProviderResponse,
    DisableProviderRequest, DisableProviderResponse,
    EnableProviderRequest, EnableProviderResponse,
    GetProviderByNameRequest, GetProviderRequest, GetProviderResponse,
    GetProviderStatsRequest, GetProviderStatsResponse,
    ListProvidersRequest, ListProvidersResponse,
    ProviderConfig as ProtoProviderConfig,
    ProviderCapabilities as ProtoProviderCapabilities,
    ProviderStatus as ProtoProviderStatus,
    ProviderType as ProtoProviderType,
    ProviderTypeConfig as ProtoProviderTypeConfig,
    RegisterProviderRequest, RegisterProviderResponse,
    ResourceLimits as ProtoResourceLimits,
    UpdateProviderRequest, UpdateProviderResponse,
    DockerConfig as ProtoDockerConfig,
    KubernetesConfig as ProtoKubernetesConfig,
};
use hodei_jobs_application::ProviderRegistry;
use hodei_jobs_domain::provider_config::{
    ProviderConfig, ProviderTypeConfig, DockerConfig, KubernetesConfig,
};
use hodei_jobs_domain::shared_kernel::{ProviderId, ProviderStatus};
use hodei_jobs_domain::worker::ProviderType;
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct ProviderManagementServiceImpl {
    registry: Arc<ProviderRegistry>,
}

impl ProviderManagementServiceImpl {
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        Self { registry }
    }

    fn domain_to_proto_provider(&self, config: &ProviderConfig) -> ProtoProviderConfig {
        ProtoProviderConfig {
            id: config.id.to_string(),
            name: config.name.clone(),
            provider_type: self.domain_to_proto_type(&config.provider_type) as i32,
            status: self.domain_to_proto_status(&config.status) as i32,
            priority: config.priority,
            max_workers: config.max_workers,
            active_workers: config.active_workers,
            capabilities: Some(self.domain_to_proto_capabilities(&config.capabilities)),
            type_config: Some(self.domain_to_proto_type_config(&config.type_config)),
            tags: config.tags.clone(),
            metadata: config.metadata.clone(),
            created_at: config.created_at.to_rfc3339(),
            updated_at: config.updated_at.to_rfc3339(),
        }
    }

    fn domain_to_proto_type(&self, pt: &ProviderType) -> ProtoProviderType {
        match pt {
            ProviderType::Docker => ProtoProviderType::Docker,
            ProviderType::Kubernetes => ProtoProviderType::Kubernetes,
            ProviderType::Fargate => ProtoProviderType::Fargate,
            ProviderType::CloudRun => ProtoProviderType::CloudRun,
            ProviderType::ContainerApps => ProtoProviderType::ContainerApps,
            ProviderType::Lambda => ProtoProviderType::Lambda,
            ProviderType::CloudFunctions => ProtoProviderType::CloudFunctions,
            ProviderType::AzureFunctions => ProtoProviderType::AzureFunctions,
            ProviderType::EC2 => ProtoProviderType::Ec2,
            ProviderType::ComputeEngine => ProtoProviderType::ComputeEngine,
            ProviderType::AzureVMs => ProtoProviderType::AzureVms,
            ProviderType::BareMetal => ProtoProviderType::BareMetal,
            ProviderType::Custom(_) => ProtoProviderType::Custom,
        }
    }

    fn proto_to_domain_type(&self, pt: ProtoProviderType) -> ProviderType {
        match pt {
            ProtoProviderType::Docker => ProviderType::Docker,
            ProtoProviderType::Kubernetes => ProviderType::Kubernetes,
            ProtoProviderType::Fargate => ProviderType::Fargate,
            ProtoProviderType::CloudRun => ProviderType::CloudRun,
            ProtoProviderType::ContainerApps => ProviderType::ContainerApps,
            ProtoProviderType::Lambda => ProviderType::Lambda,
            ProtoProviderType::CloudFunctions => ProviderType::CloudFunctions,
            ProtoProviderType::AzureFunctions => ProviderType::AzureFunctions,
            ProtoProviderType::Ec2 => ProviderType::EC2,
            ProtoProviderType::ComputeEngine => ProviderType::ComputeEngine,
            ProtoProviderType::AzureVms => ProviderType::AzureVMs,
            ProtoProviderType::BareMetal => ProviderType::BareMetal,
            ProtoProviderType::Custom | ProtoProviderType::Unspecified => {
                ProviderType::Custom("unknown".to_string())
            }
        }
    }

    fn domain_to_proto_status(&self, status: &ProviderStatus) -> ProtoProviderStatus {
        match status {
            ProviderStatus::Active => ProtoProviderStatus::Active,
            ProviderStatus::Maintenance => ProtoProviderStatus::Maintenance,
            ProviderStatus::Disabled => ProtoProviderStatus::Disabled,
            ProviderStatus::Overloaded => ProtoProviderStatus::Overloaded,
            ProviderStatus::Unhealthy => ProtoProviderStatus::Unhealthy,
            ProviderStatus::Degraded => ProtoProviderStatus::Degraded,
        }
    }

    fn domain_to_proto_capabilities(
        &self,
        caps: &hodei_jobs_domain::worker_provider::ProviderCapabilities,
    ) -> ProtoProviderCapabilities {
        ProtoProviderCapabilities {
            max_resources: Some(ProtoResourceLimits {
                max_cpu_cores: caps.max_resources.max_cpu_cores,
                max_memory_bytes: caps.max_resources.max_memory_bytes,
                max_disk_bytes: caps.max_resources.max_disk_bytes,
                max_gpu_count: caps.max_resources.max_gpu_count,
            }),
            gpu_support: caps.gpu_support,
            gpu_types: caps.gpu_types.clone(),
            architectures: caps.architectures.iter().map(|a| format!("{:?}", a)).collect(),
            runtimes: caps.runtimes.clone(),
            regions: caps.regions.clone(),
            max_execution_time_seconds: caps.max_execution_time.map(|d| d.as_secs()),
            persistent_storage: caps.persistent_storage,
            custom_networking: caps.custom_networking,
        }
    }

    fn domain_to_proto_type_config(&self, config: &ProviderTypeConfig) -> ProtoProviderTypeConfig {
        match config {
            ProviderTypeConfig::Docker(dc) => ProtoProviderTypeConfig {
                config: Some(hodei_jobs::providers::provider_type_config::Config::Docker(
                    ProtoDockerConfig {
                        socket_path: dc.socket_path.clone(),
                        default_image: dc.default_image.clone(),
                        network_mode: dc.network.clone().unwrap_or_default(),
                        dns_servers: vec![],
                        labels: std::collections::HashMap::new(),
                    },
                )),
            },
            ProviderTypeConfig::Kubernetes(kc) => ProtoProviderTypeConfig {
                config: Some(
                    hodei_jobs::providers::provider_type_config::Config::Kubernetes(
                        ProtoKubernetesConfig {
                            kubeconfig_path: kc.kubeconfig_path.clone(),
                            namespace: kc.namespace.clone(),
                            service_account: kc.service_account.clone(),
                            image_pull_secret: kc.image_pull_secrets.first().cloned().unwrap_or_default(),
                            node_selector: kc.node_selector.clone(),
                        },
                    ),
                ),
            },
            _ => ProtoProviderTypeConfig { config: None },
        }
    }

    fn proto_to_domain_type_config(
        &self,
        config: Option<ProtoProviderTypeConfig>,
        provider_type: &ProviderType,
    ) -> ProviderTypeConfig {
        match config.and_then(|c| c.config) {
            Some(hodei_jobs::providers::provider_type_config::Config::Docker(dc)) => {
                ProviderTypeConfig::Docker(DockerConfig {
                    socket_path: dc.socket_path,
                    default_image: dc.default_image,
                    network: if dc.network_mode.is_empty() { None } else { Some(dc.network_mode) },
                    ..Default::default()
                })
            }
            Some(hodei_jobs::providers::provider_type_config::Config::Kubernetes(kc)) => {
                ProviderTypeConfig::Kubernetes(KubernetesConfig {
                    kubeconfig_path: kc.kubeconfig_path,
                    namespace: kc.namespace,
                    service_account: kc.service_account,
                    image_pull_secrets: if kc.image_pull_secret.is_empty() {
                        vec![]
                    } else {
                        vec![kc.image_pull_secret]
                    },
                    node_selector: kc.node_selector,
                    ..Default::default()
                })
            }
            _ => match provider_type {
                ProviderType::Docker => ProviderTypeConfig::Docker(DockerConfig::default()),
                ProviderType::Kubernetes => {
                    ProviderTypeConfig::Kubernetes(KubernetesConfig::default())
                }
                _ => ProviderTypeConfig::Docker(DockerConfig::default()),
            },
        }
    }
}

#[tonic::async_trait]
impl ProviderManagementService for ProviderManagementServiceImpl {
    async fn register_provider(
        &self,
        request: Request<RegisterProviderRequest>,
    ) -> Result<Response<RegisterProviderResponse>, Status> {
        let req = request.into_inner();

        let provider_type = self.proto_to_domain_type(
            ProtoProviderType::try_from(req.provider_type).unwrap_or(ProtoProviderType::Docker),
        );
        let type_config = self.proto_to_domain_type_config(req.type_config, &provider_type);

        match self
            .registry
            .register_provider(req.name, provider_type, type_config)
            .await
        {
            Ok(config) => Ok(Response::new(RegisterProviderResponse {
                provider: Some(self.domain_to_proto_provider(&config)),
            })),
            Err(e) => Err(Status::invalid_argument(e.to_string())),
        }
    }

    async fn get_provider(
        &self,
        request: Request<GetProviderRequest>,
    ) -> Result<Response<GetProviderResponse>, Status> {
        let req = request.into_inner();
        let id = ProviderId::from_uuid(
            uuid::Uuid::parse_str(&req.provider_id)
                .map_err(|e| Status::invalid_argument(format!("Invalid provider ID: {}", e)))?,
        );

        match self.registry.get_provider(&id).await {
            Ok(Some(config)) => Ok(Response::new(GetProviderResponse {
                provider: Some(self.domain_to_proto_provider(&config)),
            })),
            Ok(None) => Ok(Response::new(GetProviderResponse { provider: None })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn get_provider_by_name(
        &self,
        request: Request<GetProviderByNameRequest>,
    ) -> Result<Response<GetProviderResponse>, Status> {
        let req = request.into_inner();

        match self.registry.get_provider_by_name(&req.name).await {
            Ok(Some(config)) => Ok(Response::new(GetProviderResponse {
                provider: Some(self.domain_to_proto_provider(&config)),
            })),
            Ok(None) => Ok(Response::new(GetProviderResponse { provider: None })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn list_providers(
        &self,
        request: Request<ListProvidersRequest>,
    ) -> Result<Response<ListProvidersResponse>, Status> {
        let req = request.into_inner();

        let providers = if req.only_with_capacity.unwrap_or(false) {
            self.registry.list_enabled_providers().await
        } else if let Some(pt) = req.provider_type {
            let domain_type = self.proto_to_domain_type(
                ProtoProviderType::try_from(pt).unwrap_or(ProtoProviderType::Unspecified),
            );
            self.registry.list_providers_by_type(&domain_type).await
        } else {
            self.registry.list_providers().await
        };

        match providers {
            Ok(configs) => Ok(Response::new(ListProvidersResponse {
                providers: configs
                    .iter()
                    .map(|c| self.domain_to_proto_provider(c))
                    .collect(),
            })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn update_provider(
        &self,
        request: Request<UpdateProviderRequest>,
    ) -> Result<Response<UpdateProviderResponse>, Status> {
        // For now, just return the provider as-is
        // Full update implementation would convert proto to domain and save
        let req = request.into_inner();
        Ok(Response::new(UpdateProviderResponse {
            provider: req.provider,
        }))
    }

    async fn delete_provider(
        &self,
        request: Request<DeleteProviderRequest>,
    ) -> Result<Response<DeleteProviderResponse>, Status> {
        let req = request.into_inner();
        let id = ProviderId::from_uuid(
            uuid::Uuid::parse_str(&req.provider_id)
                .map_err(|e| Status::invalid_argument(format!("Invalid provider ID: {}", e)))?,
        );

        match self.registry.delete_provider(&id).await {
            Ok(()) => Ok(Response::new(DeleteProviderResponse { success: true })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn enable_provider(
        &self,
        request: Request<EnableProviderRequest>,
    ) -> Result<Response<EnableProviderResponse>, Status> {
        let req = request.into_inner();
        let id = ProviderId::from_uuid(
            uuid::Uuid::parse_str(&req.provider_id)
                .map_err(|e| Status::invalid_argument(format!("Invalid provider ID: {}", e)))?,
        );

        match self.registry.enable_provider(&id).await {
            Ok(()) => match self.registry.get_provider(&id).await {
                Ok(Some(config)) => Ok(Response::new(EnableProviderResponse {
                    provider: Some(self.domain_to_proto_provider(&config)),
                })),
                _ => Err(Status::internal("Failed to retrieve updated provider")),
            },
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn disable_provider(
        &self,
        request: Request<DisableProviderRequest>,
    ) -> Result<Response<DisableProviderResponse>, Status> {
        let req = request.into_inner();
        let id = ProviderId::from_uuid(
            uuid::Uuid::parse_str(&req.provider_id)
                .map_err(|e| Status::invalid_argument(format!("Invalid provider ID: {}", e)))?,
        );

        match self.registry.disable_provider(&id).await {
            Ok(()) => match self.registry.get_provider(&id).await {
                Ok(Some(config)) => Ok(Response::new(DisableProviderResponse {
                    provider: Some(self.domain_to_proto_provider(&config)),
                })),
                _ => Err(Status::internal("Failed to retrieve updated provider")),
            },
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn get_provider_stats(
        &self,
        _request: Request<GetProviderStatsRequest>,
    ) -> Result<Response<GetProviderStatsResponse>, Status> {
        match self.registry.get_stats().await {
            Ok(stats) => Ok(Response::new(GetProviderStatsResponse {
                total_providers: stats.total_providers as u32,
                enabled_providers: stats.enabled_providers as u32,
                providers_with_capacity: stats.providers_with_capacity as u32,
                total_max_workers: stats.total_max_workers,
                total_active_workers: stats.total_active_workers,
                utilization_percent: stats.utilization_percent,
            })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }
}
