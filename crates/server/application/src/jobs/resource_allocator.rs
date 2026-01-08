//! Resource Allocator
//!
//! Componente dedicado para el cálculo de métricas y costos de providers.
//! Extraído de JobDispatcher para aplicar Single Responsibility Principle.
//!
//! Responsabilidades:
//! - Calcular tiempo de startup estimado por provider
//! - Calcular health score de providers
//! - Calcular costo por hora de providers
//! - Proveer métricas para scheduling decisions

use hodei_server_domain::providers::{ProviderConfig, ProviderTypeConfig};
use hodei_server_domain::scheduling::ProviderInfo;
use hodei_server_domain::workers::ProviderType;
use std::sync::Arc;
use std::time::Duration;

/// Resultado del análisis de un provider
#[derive(Debug, Clone)]
pub struct ProviderAnalysisResult {
    /// ID del provider
    pub provider_id: String,
    /// Tiempo de startup estimado
    pub estimated_startup: Duration,
    /// Health score (0.0 - 1.0)
    pub health_score: f64,
    /// Costo por hora estimado
    pub cost_per_hour: f64,
    /// Si tiene capacidad disponible
    pub has_capacity: bool,
}

/// Configuración del ResourceAllocator
#[derive(Debug, Clone)]
pub struct ResourceAllocatorConfig {
    /// Fracción del max execution time para estimar startup
    pub startup_time_fraction: f64,
    /// Tiempo mínimo de startup
    pub min_startup_time: Duration,
}

impl Default for ResourceAllocatorConfig {
    fn default() -> Self {
        Self {
            startup_time_fraction: 0.15,
            min_startup_time: Duration::from_secs(5),
        }
    }
}

/// ResourceAllocator - Componente para análisis y métricas de resources
///
/// Provee cálculos estandarizados para:
/// - Estimated startup time
/// - Health score
/// - Cost per hour
/// - Provider capacity
#[derive(Clone)]
pub struct ResourceAllocator {
    /// Configuración
    config: ResourceAllocatorConfig,
}

impl ResourceAllocator {
    /// Crear nuevo ResourceAllocator
    pub fn new(config: Option<ResourceAllocatorConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
        }
    }

    /// Analizar un provider y retornar métricas calculadas
    pub fn analyze_provider(&self, provider: &ProviderConfig) -> ProviderAnalysisResult {
        let estimated_startup = self.calculate_startup_time(provider);
        let health_score = self.calculate_health_score(provider);
        let cost_per_hour = self.calculate_provider_cost(provider);

        ProviderAnalysisResult {
            provider_id: provider.id.0.to_string(),
            estimated_startup,
            health_score,
            cost_per_hour,
            has_capacity: provider.has_capacity(),
        }
    }

    /// Convertir lista de ProviderConfigs a ProviderInfos
    pub fn configs_to_provider_infos(
        &self,
        providers: &[Arc<ProviderConfig>],
    ) -> Vec<ProviderInfo> {
        providers
            .iter()
            .map(|p| self.config_to_provider_info(p))
            .collect()
    }

    /// Convertir lista de ProviderConfigs (por referencia) a ProviderInfos
    /// GAP-GO-04: Método adicional para evitar conversión a Arc
    pub fn configs_to_provider_infos_ref(
        &self,
        providers: &[ProviderConfig],
    ) -> Vec<ProviderInfo> {
        providers
            .iter()
            .map(|p| self.config_to_provider_info(p))
            .collect()
    }

    /// Convertir un ProviderConfig a ProviderInfo
    pub fn config_to_provider_info(&self, provider: &ProviderConfig) -> ProviderInfo {
        let analysis = self.analyze_provider(provider);

        ProviderInfo {
            provider_id: provider.id.clone(),
            provider_type: provider.provider_type.clone(),
            active_workers: provider.active_workers as usize,
            max_workers: provider.max_workers as usize,
            estimated_startup_time: analysis.estimated_startup,
            health_score: analysis.health_score,
            cost_per_hour: analysis.cost_per_hour,
            gpu_support: provider.capabilities.gpu_support,
            gpu_types: provider.capabilities.gpu_types.clone(),
            regions: provider.capabilities.regions.clone(),
        }
    }

    /// Calcular tiempo de startup estimado desde ProviderConfig
    ///
    /// ## Connascence Transformation
    /// De Connascence of Position (hardcoded index) a Connascence of Type
    /// usando ProviderCapabilities.max_execution_time como fuente de verdad.
    pub fn calculate_startup_time(&self, provider: &ProviderConfig) -> Duration {
        // Try to get startup time from capabilities
        if let Some(max_exec_time) = provider.capabilities.max_execution_time {
            // Use max_execution_time as a reasonable estimate for startup
            // Typically startup is 10-20% of max execution time for well-tuned systems
            let estimated = (max_exec_time.as_secs() as f64 * self.config.startup_time_fraction)
                as u64;
            std::time::Duration::from_secs(estimated.max(5))
        } else {
            // Fallback to provider-type-based estimates
            self.default_startup_for_type(&provider.provider_type)
        }
    }

    /// Default startup times by provider type
    fn default_startup_for_type(&self, provider_type: &ProviderType) -> Duration {
        match provider_type {
            ProviderType::Docker => Duration::from_secs(3),
            ProviderType::Kubernetes => Duration::from_secs(15),
            ProviderType::Custom(_) => Duration::from_secs(8),
            ProviderType::Lambda => Duration::from_secs(2),
            ProviderType::CloudRun => Duration::from_secs(10),
            ProviderType::Fargate => Duration::from_secs(20),
            _ => Duration::from_secs(30),
        }
    }

    /// Calcular health score desde ProviderConfig
    ///
    /// ## US-27.5: Provider Health Monitor Integration
    /// Este método usa indicadores de salud desde ProviderConfig:
    /// - Estado del provider (enabled/active)
    /// - Capacidad disponible (active_workers < max_workers)
    /// - Capacidades reportadas (gpu_support, regions, etc.)
    pub fn calculate_health_score(&self, provider: &ProviderConfig) -> f64 {
        let mut score: f64 = 0.5;

        // Provider status contribution (up to 0.2)
        if provider.is_enabled() {
            score += 0.2;
        }

        // Capacity contribution (up to 0.15)
        if provider.has_capacity() {
            score += 0.15;
        }

        // Feature support contribution (up to 0.15)
        if provider.capabilities.gpu_support {
            score += 0.05;
        }
        if !provider.capabilities.regions.is_empty() {
            score += 0.05;
        }
        if provider.capabilities.persistent_storage {
            score += 0.05;
        }

        // Cap at 1.0
        score.min(1.0)
    }

    /// Calcular costo por hora desde type_config
    ///
    /// ## Connascence Transformation
    /// De Connascence of Position (hardcoded 0.0) a Connascence of Type
    /// usando el tipo de provider para determinar costo base.
    pub fn calculate_provider_cost(&self, provider: &ProviderConfig) -> f64 {
        match &provider.type_config {
            // Container providers - typically pay-per-use or fixed
            ProviderTypeConfig::Docker(_) => 0.0,
            ProviderTypeConfig::Kubernetes(k8s) => {
                let base_cost = 0.10;
                let complexity_bonus = (k8s.node_selector.len() as f64) * 0.02;
                (base_cost + complexity_bonus).min(0.50)
            }
            ProviderTypeConfig::CloudRun(_) => 0.05,
            ProviderTypeConfig::Fargate(_) => 0.15,
            ProviderTypeConfig::ContainerApps(_) => 0.10,

            // Serverless providers - typically pay-per-invocation
            ProviderTypeConfig::Lambda(lambda) => {
                let memory_factor = (lambda.memory_mb as f64 / 1024.0) * 0.017;
                let timeout_factor = (lambda.timeout_seconds as f64 / 900.0) * 0.01;
                (memory_factor + timeout_factor).min(0.10)
            }
            ProviderTypeConfig::CloudFunctions(_) => 0.05,
            ProviderTypeConfig::AzureFunctions(_) => 0.05,

            // VM providers - typically pay-per-hour
            ProviderTypeConfig::EC2(ec2) => {
                match ec2.instance_type.as_str() {
                    t if t.contains("t3") || t.contains("t2") => 0.05,
                    t if t.contains("m5") || t.contains("m6") => 0.10,
                    t if t.contains("c5") || t.contains("c6") => 0.12,
                    t if t.contains("r5") || t.contains("r6") => 0.15,
                    t if t.contains("p3") || t.contains("p4") => 3.00,
                    t if t.contains("g4") || t.contains("g5") => 2.50,
                    _ => 0.10,
                }
            }
            ProviderTypeConfig::ComputeEngine(_) => 0.08,
            ProviderTypeConfig::AzureVMs(_) => 0.08,

            // Other providers
            ProviderTypeConfig::BareMetal(_) => 0.20,
            ProviderTypeConfig::Custom(_) => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::providers::{ProviderConfig, ProviderTypeConfig};
    use hodei_server_domain::workers::ProviderType;

    fn make_test_provider(
        provider_type: ProviderType,
        type_config: ProviderTypeConfig,
    ) -> ProviderConfig {
        ProviderConfig::new(
            "test-provider".to_string(),
            provider_type,
            type_config,
        )
    }

    #[tokio::test]
    async fn test_resource_allocator_config_default() {
        let config = ResourceAllocatorConfig::default();
        assert_eq!(config.startup_time_fraction, 0.15);
        assert_eq!(config.min_startup_time, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_cost_docker_is_zero() {
        let allocator = ResourceAllocator::new(None);
        let provider = make_test_provider(
            ProviderType::Docker,
            ProviderTypeConfig::Docker(Default::default()),
        );
        let cost = allocator.calculate_provider_cost(&provider);
        assert_eq!(cost, 0.0);
    }

    #[tokio::test]
    async fn test_analyze_provider() {
        let allocator = ResourceAllocator::new(None);
        let provider = make_test_provider(
            ProviderType::Kubernetes,
            ProviderTypeConfig::Kubernetes(Default::default()),
        );
        let result = allocator.analyze_provider(&provider);

        assert!(result.estimated_startup > Duration::ZERO);
        assert!(result.health_score >= 0.0 && result.health_score <= 1.0);
        assert!(result.cost_per_hour >= 0.0);
        assert!(!result.provider_id.is_empty());
    }
}
