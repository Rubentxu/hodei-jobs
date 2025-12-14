// WorkerProvider Trait - Abstracción para crear workers on-demand

use crate::shared_kernel::{ProviderId, WorkerState, DomainError};
use crate::worker::{
    ProviderType, ProviderCategory, WorkerSpec, WorkerHandle, 
    ResourceRequirements, Architecture,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Estado de salud del provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Provider completamente operativo
    Healthy,
    /// Provider con degradación (funciona pero con problemas)
    Degraded { reason: String },
    /// Provider no disponible
    Unhealthy { reason: String },
    /// Estado desconocido (sin health check reciente)
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Capacidades que ofrece un provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Recursos máximos soportados
    pub max_resources: ResourceLimits,
    /// Soporta GPU
    pub gpu_support: bool,
    /// Tipos de GPU disponibles
    pub gpu_types: Vec<String>,
    /// Arquitecturas soportadas
    pub architectures: Vec<Architecture>,
    /// Runtimes soportados (para serverless)
    pub runtimes: Vec<String>,
    /// Regiones disponibles
    pub regions: Vec<String>,
    /// Tiempo máximo de ejecución
    pub max_execution_time: Option<Duration>,
    /// Soporta persistent storage
    pub persistent_storage: bool,
    /// Soporta networking custom
    pub custom_networking: bool,
    /// Características específicas del provider
    pub features: HashMap<String, serde_json::Value>,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            max_resources: ResourceLimits::default(),
            gpu_support: false,
            gpu_types: vec![],
            architectures: vec![Architecture::Amd64],
            runtimes: vec!["shell".to_string()],
            regions: vec!["local".to_string()],
            max_execution_time: Some(Duration::from_secs(3600)),
            persistent_storage: false,
            custom_networking: false,
            features: HashMap::new(),
        }
    }
}

/// Límites de recursos del provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_cpu_cores: f64,
    pub max_memory_bytes: i64,
    pub max_disk_bytes: i64,
    pub max_gpu_count: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_cores: 4.0,
            max_memory_bytes: 8 * 1024 * 1024 * 1024, // 8GB
            max_disk_bytes: 100 * 1024 * 1024 * 1024,  // 100GB
            max_gpu_count: 0,
        }
    }
}

/// Requerimientos de un job para selección de provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequirements {
    /// Recursos mínimos necesarios
    pub resources: ResourceRequirements,
    /// Arquitectura requerida
    pub architecture: Option<Architecture>,
    /// Capabilities requeridas
    pub required_capabilities: Vec<String>,
    /// Labels requeridos en el worker
    pub required_labels: HashMap<String, String>,
    /// Regiones permitidas
    pub allowed_regions: Vec<String>,
    /// Timeout máximo del job
    pub timeout: Option<Duration>,
    /// Preferencia de categoría de provider
    pub preferred_category: Option<ProviderCategory>,
}

impl Default for JobRequirements {
    fn default() -> Self {
        Self {
            resources: ResourceRequirements::default(),
            architecture: None,
            required_capabilities: vec![],
            required_labels: HashMap::new(),
            allowed_regions: vec![],
            timeout: None,
            preferred_category: None,
        }
    }
}

/// Estimación de costo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Moneda (USD, EUR, etc.)
    pub currency: String,
    /// Monto estimado
    pub amount: f64,
    /// Unidad de costo
    pub unit: CostUnit,
    /// Desglose por componente
    pub breakdown: HashMap<String, f64>,
}

impl CostEstimate {
    pub fn zero() -> Self {
        Self {
            currency: "USD".to_string(),
            amount: 0.0,
            unit: CostUnit::PerHour,
            breakdown: HashMap::new(),
        }
    }
}

/// Unidad de cobro
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CostUnit {
    PerSecond,
    PerMinute,
    PerHour,
    PerInvocation,
    PerGBSecond,
}

/// Log entry del worker (infraestructura, no job)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Errores específicos del provider
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Worker not found: {0}")]
    WorkerNotFound(String),

    #[error("Provisioning failed: {0}")]
    ProvisioningFailed(String),

    #[error("Provisioning timeout")]
    ProvisioningTimeout,

    #[error("Operation timeout: {0}")]
    Timeout(String),

    #[error("Provider not ready: {0}")]
    NotReady(String),

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Provider specific error: {0}")]
    ProviderSpecific(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<ProviderError> for DomainError {
    fn from(err: ProviderError) -> Self {
        match err {
            ProviderError::WorkerNotFound(id) => DomainError::WorkerProvisioningFailed {
                message: format!("Worker not found: {}", id),
            },
            ProviderError::ProvisioningFailed(msg) => DomainError::WorkerProvisioningFailed {
                message: msg,
            },
            ProviderError::ProvisioningTimeout => DomainError::WorkerProvisioningTimeout,
            _ => DomainError::InfrastructureError {
                message: err.to_string(),
            },
        }
    }
}

/// Trait principal para todos los providers de workers.
/// 
/// Implementaciones: DockerProvider, KubernetesProvider, LambdaProvider,
/// FargateProvider, CloudRunProvider, EC2Provider, etc.
#[async_trait]
pub trait WorkerProvider: Send + Sync {
    /// Identificador único del provider configurado
    fn provider_id(&self) -> &ProviderId;

    /// Tipo de provider
    fn provider_type(&self) -> ProviderType;

    /// Categoría del provider (Container, Serverless, VM)
    fn category(&self) -> ProviderCategory {
        self.provider_type().category()
    }

    /// Capacidades que pueden ofrecer los workers
    fn capabilities(&self) -> &ProviderCapabilities;

    /// Crear un Worker con la especificación dada
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError>;

    /// Obtener estado actual del worker
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError>;

    /// Destruir el worker (cleanup de recursos)
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError>;

    /// Obtener logs del worker (infraestructura, no job)
    async fn get_worker_logs(
        &self,
        handle: &WorkerHandle,
        tail: Option<u32>,
    ) -> Result<Vec<LogEntry>, ProviderError>;

    /// Health check del provider
    async fn health_check(&self) -> Result<HealthStatus, ProviderError>;

    /// Validar que el provider puede ejecutar un job con estos requirements
    fn can_fulfill(&self, requirements: &JobRequirements) -> bool {
        let caps = self.capabilities();

        // Verificar recursos
        if requirements.resources.cpu_cores > caps.max_resources.max_cpu_cores {
            return false;
        }
        if requirements.resources.memory_bytes > caps.max_resources.max_memory_bytes {
            return false;
        }
        if requirements.resources.gpu_count > 0 && !caps.gpu_support {
            return false;
        }

        // Verificar arquitectura
        if let Some(ref arch) = requirements.architecture {
            if !caps.architectures.contains(arch) {
                return false;
            }
        }

        // Verificar capabilities
        for cap in &requirements.required_capabilities {
            if !caps.runtimes.contains(cap) && !caps.features.contains_key(cap) {
                return false;
            }
        }

        // Verificar regiones
        if !requirements.allowed_regions.is_empty() {
            let has_region = requirements
                .allowed_regions
                .iter()
                .any(|r| caps.regions.contains(r));
            if !has_region {
                return false;
            }
        }

        // Verificar timeout
        if let (Some(job_timeout), Some(max_time)) = (requirements.timeout, caps.max_execution_time)
        {
            if job_timeout > max_time {
                return false;
            }
        }

        true
    }

    /// Estimar costo de ejecutar un job (opcional)
    fn estimate_cost(&self, _spec: &WorkerSpec, _duration: Duration) -> Option<CostEstimate> {
        None
    }

    /// Tiempo estimado de provisioning
    fn estimated_startup_time(&self) -> Duration {
        Duration::from_secs(30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_default() {
        let status = HealthStatus::default();
        assert_eq!(status, HealthStatus::Unknown);
    }

    #[test]
    fn test_provider_capabilities_default() {
        let caps = ProviderCapabilities::default();
        assert!(!caps.gpu_support);
        assert!(caps.architectures.contains(&Architecture::Amd64));
    }

    #[test]
    fn test_job_requirements_fulfillment() {
        let caps = ProviderCapabilities {
            max_resources: ResourceLimits {
                max_cpu_cores: 4.0,
                max_memory_bytes: 8 * 1024 * 1024 * 1024,
                max_disk_bytes: 100 * 1024 * 1024 * 1024,
                max_gpu_count: 0,
            },
            gpu_support: false,
            architectures: vec![Architecture::Amd64],
            runtimes: vec!["docker".to_string()],
            ..Default::default()
        };

        // Requirements que caben
        let req = JobRequirements {
            resources: ResourceRequirements {
                cpu_cores: 2.0,
                memory_bytes: 4 * 1024 * 1024 * 1024,
                ..Default::default()
            },
            ..Default::default()
        };

        // Simular can_fulfill manualmente
        assert!(req.resources.cpu_cores <= caps.max_resources.max_cpu_cores);
        assert!(req.resources.memory_bytes <= caps.max_resources.max_memory_bytes);
    }

    #[test]
    fn test_cost_estimate_zero() {
        let cost = CostEstimate::zero();
        assert_eq!(cost.amount, 0.0);
        assert_eq!(cost.currency, "USD");
    }

    #[test]
    fn test_provider_error_conversion() {
        let err = ProviderError::ProvisioningFailed("test".to_string());
        let domain_err: DomainError = err.into();
        assert!(matches!(domain_err, DomainError::WorkerProvisioningFailed { .. }));
    }
}
