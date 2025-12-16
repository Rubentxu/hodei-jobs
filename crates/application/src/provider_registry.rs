// Provider Registry Service
// Gestiona el registro y selección de providers

use chrono::Utc;
use hodei_jobs_domain::event_bus::EventBus;
use hodei_jobs_domain::events::DomainEvent;
use hodei_jobs_domain::provider_config::{
    ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
};
use hodei_jobs_domain::shared_kernel::{DomainError, ProviderId, ProviderStatus, Result};
use hodei_jobs_domain::worker::ProviderType;
use hodei_jobs_domain::worker_provider::JobRequirements;
use std::sync::Arc;

/// Servicio de registro y gestión de providers
pub struct ProviderRegistry {
    repository: Arc<dyn ProviderConfigRepository>,
    event_bus: Option<Arc<dyn EventBus>>,
}

impl ProviderRegistry {
    pub fn new(repository: Arc<dyn ProviderConfigRepository>) -> Self {
        Self {
            repository,
            event_bus: None,
        }
    }

    /// Create with EventBus for event publishing
    pub fn with_event_bus(
        repository: Arc<dyn ProviderConfigRepository>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            repository,
            event_bus: Some(event_bus),
        }
    }

    /// Publish ProviderHealthChanged event if EventBus is configured
    async fn publish_health_changed(
        &self,
        provider_id: &ProviderId,
        old_status: ProviderStatus,
        new_status: ProviderStatus,
    ) {
        if let Some(ref event_bus) = self.event_bus {
            let event = DomainEvent::ProviderHealthChanged {
                provider_id: provider_id.clone(),
                old_status,
                new_status,
                occurred_at: Utc::now(),
                correlation_id: None,
                actor: Some("provider-registry".to_string()),
            };
            if let Err(e) = event_bus.publish(&event).await {
                tracing::warn!("Failed to publish ProviderHealthChanged event: {}", e);
            }
        }
    }

    /// Registrar un nuevo provider
    pub async fn register_provider(
        &self,
        name: String,
        provider_type: ProviderType,
        type_config: ProviderTypeConfig,
    ) -> Result<ProviderConfig> {
        // Verificar que no exista un provider con el mismo nombre
        if self.repository.exists_by_name(&name).await? {
            return Err(DomainError::InvalidProviderConfig {
                message: format!("Provider with name '{}' already exists", name),
            });
        }

        let config = ProviderConfig::new(name, provider_type, type_config);
        self.repository.save(&config).await?;

        Ok(config)
    }

    /// Registrar provider con configuración completa
    pub async fn register_provider_with_config(
        &self,
        config: ProviderConfig,
    ) -> Result<ProviderConfig> {
        // Verificar que no exista un provider con el mismo nombre
        if self.repository.exists_by_name(&config.name).await? {
            return Err(DomainError::InvalidProviderConfig {
                message: format!("Provider with name '{}' already exists", config.name),
            });
        }

        self.repository.save(&config).await?;
        Ok(config)
    }

    /// Obtener provider por ID
    pub async fn get_provider(&self, id: &ProviderId) -> Result<Option<ProviderConfig>> {
        self.repository.find_by_id(id).await
    }

    /// Obtener provider por nombre
    pub async fn get_provider_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
        self.repository.find_by_name(name).await
    }

    /// Listar todos los providers
    pub async fn list_providers(&self) -> Result<Vec<ProviderConfig>> {
        self.repository.find_all().await
    }

    /// Listar providers habilitados
    pub async fn list_enabled_providers(&self) -> Result<Vec<ProviderConfig>> {
        self.repository.find_enabled().await
    }

    pub async fn list_providers_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
        self.repository.find_with_capacity().await
    }

    /// Listar providers por tipo
    pub async fn list_providers_by_type(
        &self,
        provider_type: &ProviderType,
    ) -> Result<Vec<ProviderConfig>> {
        self.repository.find_by_type(provider_type).await
    }

    /// Habilitar un provider
    pub async fn enable_provider(&self, id: &ProviderId) -> Result<()> {
        let mut config = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: id.clone(),
            })?;

        let old_status = config.status.clone();
        config.status = ProviderStatus::Active;
        config.updated_at = chrono::Utc::now();
        self.repository.update(&config).await?;

        // Publish event if status changed
        if old_status != ProviderStatus::Active {
            self.publish_health_changed(id, old_status, ProviderStatus::Active)
                .await;
        }
        Ok(())
    }

    /// Deshabilitar un provider
    pub async fn disable_provider(&self, id: &ProviderId) -> Result<()> {
        let mut config = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: id.clone(),
            })?;

        let old_status = config.status.clone();
        config.status = ProviderStatus::Disabled;
        config.updated_at = chrono::Utc::now();
        self.repository.update(&config).await?;

        // Publish event if status changed
        if old_status != ProviderStatus::Disabled {
            self.publish_health_changed(id, old_status, ProviderStatus::Disabled)
                .await;
        }
        Ok(())
    }

    /// Update provider status with event publishing
    pub async fn update_provider_status(
        &self,
        id: &ProviderId,
        new_status: ProviderStatus,
    ) -> Result<()> {
        let mut config = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: id.clone(),
            })?;

        let old_status = config.status.clone();
        if old_status == new_status {
            return Ok(()); // No change needed
        }

        config.status = new_status.clone();
        config.updated_at = chrono::Utc::now();
        self.repository.update(&config).await?;

        self.publish_health_changed(id, old_status, new_status).await;
        Ok(())
    }

    /// Actualizar configuración de un provider
    pub async fn update_provider(&self, config: ProviderConfig) -> Result<()> {
        self.repository.update(&config).await
    }

    /// Eliminar un provider
    pub async fn delete_provider(&self, id: &ProviderId) -> Result<()> {
        self.repository.delete(id).await
    }

    /// Seleccionar el mejor provider para los requisitos dados
    pub async fn select_best_provider(
        &self,
        requirements: &JobRequirements,
    ) -> Result<Option<ProviderConfig>> {
        let available = self.repository.find_with_capacity().await?;

        // Filtrar providers que pueden cumplir los requisitos
        let mut candidates: Vec<_> = available
            .into_iter()
            .filter(|p| self.can_fulfill_requirements(p, requirements))
            .collect();

        if candidates.is_empty() {
            return Ok(None);
        }

        // Ordenar por prioridad (mayor primero) y luego por carga (menor primero)
        candidates.sort_by(|a, b| {
            let priority_cmp = b.priority.cmp(&a.priority);
            if priority_cmp == std::cmp::Ordering::Equal {
                // Menor carga relativa primero
                let load_a = a.active_workers as f64 / a.max_workers.max(1) as f64;
                let load_b = b.active_workers as f64 / b.max_workers.max(1) as f64;
                load_a.partial_cmp(&load_b).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                priority_cmp
            }
        });

        Ok(candidates.into_iter().next())
    }

    /// Verificar si un provider puede cumplir los requisitos
    fn can_fulfill_requirements(
        &self,
        provider: &ProviderConfig,
        requirements: &JobRequirements,
    ) -> bool {
        let caps = &provider.capabilities;

        // Verificar CPU
        if caps.max_resources.max_cpu_cores < requirements.resources.cpu_cores {
            return false;
        }

        // Verificar memoria
        if caps.max_resources.max_memory_bytes < requirements.resources.memory_bytes {
            return false;
        }

        // Verificar GPU si es requerido
        if requirements.resources.gpu_count > 0 && !caps.gpu_support {
            return false;
        }

        // Verificar timeout
        if let Some(required_timeout) = requirements.timeout {
            if let Some(max_timeout) = caps.max_execution_time {
                if max_timeout < required_timeout {
                    return false;
                }
            }
        }

        // Verificar arquitectura si es especificada
        if let Some(ref required_arch) = requirements.architecture {
            if !caps.architectures.contains(required_arch) {
                return false;
            }
        }

        true
    }

    /// Incrementar contador de workers activos
    pub async fn increment_active_workers(&self, id: &ProviderId) -> Result<()> {
        let mut config = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: id.clone(),
            })?;

        config.increment_workers();
        self.repository.update(&config).await
    }

    /// Decrementar contador de workers activos
    pub async fn decrement_active_workers(&self, id: &ProviderId) -> Result<()> {
        let mut config = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: id.clone(),
            })?;

        config.decrement_workers();
        self.repository.update(&config).await
    }

    /// Obtener estadísticas de providers
    pub async fn get_stats(&self) -> Result<ProviderRegistryStats> {
        let all = self.repository.find_all().await?;
        let enabled = self.repository.find_enabled().await?;
        let with_capacity = self.repository.find_with_capacity().await?;

        let total_max_workers: u32 = all.iter().map(|p| p.max_workers).sum();
        let total_active_workers: u32 = all.iter().map(|p| p.active_workers).sum();

        Ok(ProviderRegistryStats {
            total_providers: all.len(),
            enabled_providers: enabled.len(),
            providers_with_capacity: with_capacity.len(),
            total_max_workers,
            total_active_workers,
            utilization_percent: if total_max_workers > 0 {
                (total_active_workers as f64 / total_max_workers as f64) * 100.0
            } else {
                0.0
            },
        })
    }
}

/// Estadísticas del registry de providers
#[derive(Debug, Clone)]
pub struct ProviderRegistryStats {
    pub total_providers: usize,
    pub enabled_providers: usize,
    pub providers_with_capacity: usize,
    pub total_max_workers: u32,
    pub total_active_workers: u32,
    pub utilization_percent: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_jobs_domain::provider_config::DockerConfig;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    /// Mock repository para tests
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

    #[async_trait::async_trait]
    impl ProviderConfigRepository for MockProviderConfigRepository {
        async fn save(&self, config: &ProviderConfig) -> Result<()> {
            self.configs.write().await.insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>> {
            Ok(self.configs.read().await.get(id).cloned())
        }

        async fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>> {
            Ok(self.configs.read().await.values().find(|c| c.name == name).cloned())
        }

        async fn find_by_type(&self, pt: &ProviderType) -> Result<Vec<ProviderConfig>> {
            Ok(self.configs.read().await.values()
                .filter(|c| &c.provider_type == pt)
                .cloned()
                .collect())
        }

        async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
            Ok(self.configs.read().await.values()
                .filter(|c| c.status == ProviderStatus::Active)
                .cloned()
                .collect())
        }

        async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
            Ok(self.configs.read().await.values()
                .filter(|c| c.status == ProviderStatus::Active && c.has_capacity())
                .cloned()
                .collect())
        }

        async fn find_all(&self) -> Result<Vec<ProviderConfig>> {
            Ok(self.configs.read().await.values().cloned().collect())
        }

        async fn update(&self, config: &ProviderConfig) -> Result<()> {
            self.configs.write().await.insert(config.id.clone(), config.clone());
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

    fn create_docker_config() -> ProviderTypeConfig {
        ProviderTypeConfig::Docker(DockerConfig::default())
    }

    #[tokio::test]
    async fn test_register_provider() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = ProviderRegistry::new(repo);

        let config = registry
            .register_provider(
                "docker-local".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        assert_eq!(config.name, "docker-local");
        assert_eq!(config.provider_type, ProviderType::Docker);
    }

    #[tokio::test]
    async fn test_register_duplicate_name_fails() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = ProviderRegistry::new(repo);

        registry
            .register_provider(
                "docker-local".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        let result = registry
            .register_provider(
                "docker-local".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_enable_disable_provider() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = ProviderRegistry::new(repo);

        let config = registry
            .register_provider(
                "docker-local".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        // Deshabilitar
        registry.disable_provider(&config.id).await.unwrap();
        let disabled = registry.get_provider(&config.id).await.unwrap().unwrap();
        assert_eq!(disabled.status, ProviderStatus::Disabled);

        // Habilitar
        registry.enable_provider(&config.id).await.unwrap();
        let enabled = registry.get_provider(&config.id).await.unwrap().unwrap();
        assert_eq!(enabled.status, ProviderStatus::Active);
    }

    #[tokio::test]
    async fn test_select_best_provider() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = ProviderRegistry::new(repo);

        // Registrar providers con diferentes prioridades
        let _low_priority = registry
            .register_provider(
                "low-priority".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        let high_priority = ProviderConfig::new(
            "high-priority".to_string(),
            ProviderType::Docker,
            create_docker_config(),
        )
        .with_priority(100);
        registry.register_provider_with_config(high_priority.clone()).await.unwrap();

        let requirements = JobRequirements::default();
        let selected = registry.select_best_provider(&requirements).await.unwrap();

        assert!(selected.is_some());
        assert_eq!(selected.unwrap().name, "high-priority");
    }

    #[tokio::test]
    async fn test_get_stats() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let registry = ProviderRegistry::new(repo);

        registry
            .register_provider(
                "docker-1".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        registry
            .register_provider(
                "docker-2".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        let stats = registry.get_stats().await.unwrap();
        assert_eq!(stats.total_providers, 2);
        assert_eq!(stats.enabled_providers, 2);
    }

    // Mock EventBus for testing event publishing
    use async_trait::async_trait;
    use futures::stream::BoxStream;
    use hodei_jobs_domain::event_bus::EventBusError;
    use std::sync::Mutex;

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

    #[tokio::test]
    async fn test_enable_disable_publishes_provider_health_changed_event() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let event_bus = Arc::new(MockEventBus::new());
        let registry = ProviderRegistry::with_event_bus(repo, event_bus.clone());

        let config = registry
            .register_provider(
                "docker-local".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        // Disable provider - should publish event
        registry.disable_provider(&config.id).await.unwrap();

        let events = event_bus.published.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::ProviderHealthChanged {
                provider_id,
                old_status,
                new_status,
                ..
            } => {
                assert_eq!(provider_id, &config.id);
                assert_eq!(*old_status, ProviderStatus::Active);
                assert_eq!(*new_status, ProviderStatus::Disabled);
            }
            _ => panic!("Expected ProviderHealthChanged event"),
        }
        drop(events);

        // Enable provider - should publish another event
        registry.enable_provider(&config.id).await.unwrap();

        let events = event_bus.published.lock().unwrap();
        assert_eq!(events.len(), 2);
        match &events[1] {
            DomainEvent::ProviderHealthChanged {
                old_status,
                new_status,
                ..
            } => {
                assert_eq!(*old_status, ProviderStatus::Disabled);
                assert_eq!(*new_status, ProviderStatus::Active);
            }
            _ => panic!("Expected ProviderHealthChanged event"),
        }
    }

    #[tokio::test]
    async fn test_update_provider_status_publishes_event() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let event_bus = Arc::new(MockEventBus::new());
        let registry = ProviderRegistry::with_event_bus(repo, event_bus.clone());

        let config = registry
            .register_provider(
                "docker-local".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        // Update to Unhealthy status
        registry
            .update_provider_status(&config.id, ProviderStatus::Unhealthy)
            .await
            .unwrap();

        let events = event_bus.published.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::ProviderHealthChanged {
                old_status,
                new_status,
                ..
            } => {
                assert_eq!(*old_status, ProviderStatus::Active);
                assert_eq!(*new_status, ProviderStatus::Unhealthy);
            }
            _ => panic!("Expected ProviderHealthChanged event"),
        }
    }

    #[tokio::test]
    async fn test_no_event_when_status_unchanged() {
        let repo = Arc::new(MockProviderConfigRepository::new());
        let event_bus = Arc::new(MockEventBus::new());
        let registry = ProviderRegistry::with_event_bus(repo, event_bus.clone());

        let config = registry
            .register_provider(
                "docker-local".to_string(),
                ProviderType::Docker,
                create_docker_config(),
            )
            .await
            .unwrap();

        // Try to update to same status (Active -> Active)
        registry
            .update_provider_status(&config.id, ProviderStatus::Active)
            .await
            .unwrap();

        // No event should be published
        let events = event_bus.published.lock().unwrap();
        assert!(events.is_empty());
    }
}
