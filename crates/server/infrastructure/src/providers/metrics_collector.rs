//! Real Performance Metrics Collector
//!
//! Recolecta métricas reales de los providers en lugar de usar datos simulados.

use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::{ProviderPerformanceMetrics, ResourceUsageStats};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Servicio para recolectar métricas reales de performance de providers
#[derive(Clone)]
pub struct ProviderMetricsCollector {
    /// Métricas por provider
    metrics: Arc<RwLock<HashMap<ProviderId, ProviderMetrics>>>,
    /// Provider ID this collector belongs to
    provider_id: ProviderId,
}

/// Métricas internas de un provider
#[derive(Debug, Clone)]
struct ProviderMetrics {
    /// Startup times registrados
    pub startup_times: Vec<Duration>,
    /// Workers creados exitosamente
    pub successful_creations: u64,
    /// Workers que fallaron
    pub failed_creations: u64,
    /// Tiempo de la última actualización
    pub last_update: Instant,
    /// Costos calculados por hora
    pub cost_per_hour: f64,
    /// Errores por minuto
    pub errors_per_minute: f64,
    /// Uso promedio de recursos
    pub avg_resource_usage: ResourceUsageStats,
}

impl ProviderMetrics {
    fn new() -> Self {
        Self {
            startup_times: Vec::new(),
            successful_creations: 0,
            failed_creations: 0,
            last_update: Instant::now(),
            cost_per_hour: 0.0,
            errors_per_minute: 0.0,
            avg_resource_usage: ResourceUsageStats::default(),
        }
    }
}

impl ProviderMetricsCollector {
    /// Crear nuevo collector para un provider específico
    pub fn new(provider_id: ProviderId) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            provider_id,
        }
    }

    /// Registrar creación de worker
    pub async fn record_worker_creation(&self, startup_time: Duration, success: bool) {
        let mut metrics = self.metrics.write().await;

        let provider_metrics = metrics
            .entry(self.provider_id.clone())
            .or_insert_with(ProviderMetrics::new);

        // Registrar startup time
        provider_metrics.startup_times.push(startup_time);

        // Mantener solo los últimos 100 samples
        if provider_metrics.startup_times.len() > 100 {
            provider_metrics.startup_times.remove(0);
        }

        // Actualizar contadores
        if success {
            provider_metrics.successful_creations += 1;
        } else {
            provider_metrics.failed_creations += 1;
        }

        provider_metrics.last_update = Instant::now();

        info!(
            provider_id = %self.provider_id,
            startup_time_ms = %startup_time.as_millis(),
            success,
            "Worker creation recorded"
        );
    }

    /// Obtener métricas actuales del provider
    pub fn get_metrics(&self) -> ProviderPerformanceMetrics {
        let metrics = self.metrics.blocking_read();
        let provider_metrics = match metrics.get(&self.provider_id) {
            Some(m) => m.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                ProviderMetrics::new()
            }
        };

        // Calcular tasa de éxito
        let total_attempts =
            provider_metrics.successful_creations + provider_metrics.failed_creations;
        let success_rate = if total_attempts > 0 {
            provider_metrics.successful_creations as f64 / total_attempts as f64
        } else {
            1.0
        };

        // Calcular throughput (workers por minuto)
        let elapsed_minutes = provider_metrics.last_update.elapsed().as_secs() as f64 / 60.0;
        let throughput = if elapsed_minutes > 0.0 {
            provider_metrics.successful_creations as f64 / elapsed_minutes
        } else {
            0.0
        };

        ProviderPerformanceMetrics {
            startup_times: provider_metrics.startup_times.clone(),
            success_rate,
            avg_cost_per_hour: provider_metrics.cost_per_hour,
            workers_per_minute: throughput,
            errors_per_minute: provider_metrics.errors_per_minute,
            avg_resource_usage: provider_metrics.avg_resource_usage.clone(),
        }
    }

    /// Async version for use in async contexts
    pub async fn get_metrics_async(&self) -> ProviderPerformanceMetrics {
        let metrics = self.metrics.read().await;
        let provider_metrics = match metrics.get(&self.provider_id) {
            Some(m) => m.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                ProviderMetrics::new()
            }
        };

        // Calcular tasa de éxito
        let total_attempts =
            provider_metrics.successful_creations + provider_metrics.failed_creations;
        let success_rate = if total_attempts > 0 {
            provider_metrics.successful_creations as f64 / total_attempts as f64
        } else {
            1.0
        };

        // Calcular throughput (workers por minuto)
        let elapsed_minutes = provider_metrics.last_update.elapsed().as_secs() as f64 / 60.0;
        let throughput = if elapsed_minutes > 0.0 {
            provider_metrics.successful_creations as f64 / elapsed_minutes
        } else {
            0.0
        };

        ProviderPerformanceMetrics {
            startup_times: provider_metrics.startup_times.clone(),
            success_rate,
            avg_cost_per_hour: provider_metrics.cost_per_hour,
            workers_per_minute: throughput,
            errors_per_minute: provider_metrics.errors_per_minute,
            avg_resource_usage: provider_metrics.avg_resource_usage.clone(),
        }
    }

    /// Obtener historial de startup times
    pub fn get_startup_times(&self) -> Vec<Duration> {
        let metrics = self.metrics.blocking_read();
        match metrics.get(&self.provider_id) {
            Some(m) => m.startup_times.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                Vec::new()
            }
        }
    }

    /// Async version for use in async contexts
    pub async fn get_startup_times_async(&self) -> Vec<Duration> {
        let metrics = self.metrics.read().await;
        match metrics.get(&self.provider_id) {
            Some(m) => m.startup_times.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                Vec::new()
            }
        }
    }

    /// Obtener uso promedio de recursos
    pub fn get_average_resource_usage(&self) -> ResourceUsageStats {
        let metrics = self.metrics.blocking_read();
        match metrics.get(&self.provider_id) {
            Some(m) => m.avg_resource_usage.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                ResourceUsageStats::default()
            }
        }
    }

    /// Async version for use in async contexts
    pub async fn get_average_resource_usage_async(&self) -> ResourceUsageStats {
        let metrics = self.metrics.read().await;
        match metrics.get(&self.provider_id) {
            Some(m) => m.avg_resource_usage.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                ResourceUsageStats::default()
            }
        }
    }

    /// Calcular health score basado en métricas (0.0 - 1.0)
    pub fn calculate_health_score(&self) -> f64 {
        let metrics = self.metrics.blocking_read();
        let provider_metrics = match metrics.get(&self.provider_id) {
            Some(m) => m.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                ProviderMetrics::new()
            }
        };

        // Calcular score basado en:
        // - Tasa de éxito (peso 50%)
        // - Tiempo de startup (peso 30%)
        // - Errores recientes (peso 20%)

        let total_attempts =
            provider_metrics.successful_creations + provider_metrics.failed_creations;

        if total_attempts == 0 {
            return 0.95; // Sin datos, asumir salud buena
        }

        let success_score = provider_metrics.successful_creations as f64 / total_attempts as f64;

        // Score de tiempo de startup (menos tiempo = mejor score)
        let avg_startup = if provider_metrics.startup_times.is_empty() {
            Duration::from_secs(5)
        } else {
            let sum: Duration = provider_metrics.startup_times.iter().sum();
            sum / provider_metrics.startup_times.len() as u32
        };

        // Normalizar a 0-1 (5 segundos = score 1.0, 30 segundos = score 0.0)
        let startup_score = if avg_startup.as_secs() <= 5 {
            1.0
        } else if avg_startup.as_secs() >= 30 {
            0.0
        } else {
            1.0 - ((avg_startup.as_secs() - 5) as f64 / 25.0)
        };

        // Score de errores (menos errores = mejor score)
        let error_rate = if total_attempts > 0 {
            provider_metrics.failed_creations as f64 / total_attempts as f64
        } else {
            0.0
        };
        let error_score = 1.0 - error_rate;

        // Combinar scores con pesos
        (success_score * 0.5) + (startup_score * 0.3) + (error_score * 0.2)
    }

    /// Async version for use in async contexts
    pub async fn calculate_health_score_async(&self) -> f64 {
        let metrics = self.metrics.read().await;
        let provider_metrics = match metrics.get(&self.provider_id) {
            Some(m) => m.clone(),
            None => {
                warn!("No metrics found for provider {}", self.provider_id);
                ProviderMetrics::new()
            }
        };

        // Calcular score basado en:
        // - Tasa de éxito (peso 50%)
        // - Tiempo de startup (peso 30%)
        // - Errores recientes (peso 20%)

        let total_attempts =
            provider_metrics.successful_creations + provider_metrics.failed_creations;

        if total_attempts == 0 {
            return 0.95; // Sin datos, asumir salud buena
        }

        let success_score = provider_metrics.successful_creations as f64 / total_attempts as f64;

        // Score de tiempo de startup (menos tiempo = mejor score)
        let avg_startup = if provider_metrics.startup_times.is_empty() {
            Duration::from_secs(5)
        } else {
            let sum: Duration = provider_metrics.startup_times.iter().sum();
            sum / provider_metrics.startup_times.len() as u32
        };

        // Normalizar a 0-1 (5 segundos = score 1.0, 30 segundos = score 0.0)
        let startup_score = if avg_startup.as_secs() <= 5 {
            1.0
        } else if avg_startup.as_secs() >= 30 {
            0.0
        } else {
            1.0 - ((avg_startup.as_secs() - 5) as f64 / 25.0)
        };

        // Score de errores (menos errores = mejor score)
        let error_rate = if total_attempts > 0 {
            provider_metrics.failed_creations as f64 / total_attempts as f64
        } else {
            0.0
        };
        let error_score = 1.0 - error_rate;

        // Combinar scores con pesos
        (success_score * 0.5) + (startup_score * 0.3) + (error_score * 0.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::shared_kernel::ProviderId;

    #[tokio::test]
    async fn test_record_successful_creation() {
        let provider_id = ProviderId::new();
        let collector = ProviderMetricsCollector::new(provider_id.clone());

        collector
            .record_worker_creation(Duration::from_secs(5), true)
            .await;

        let metrics = collector.get_metrics_async().await;
        assert_eq!(metrics.success_rate, 1.0);
        assert_eq!(metrics.startup_times.len(), 1);
    }

    #[tokio::test]
    async fn test_record_failed_creation() {
        let provider_id = ProviderId::new();
        let collector = ProviderMetricsCollector::new(provider_id.clone());

        // Crear uno exitoso y uno fallido
        collector
            .record_worker_creation(Duration::from_secs(5), true)
            .await;
        collector
            .record_worker_creation(Duration::from_secs(10), false)
            .await;

        let metrics = collector.get_metrics_async().await;
        assert!((metrics.success_rate - 0.5).abs() < 0.01);
        assert_eq!(metrics.startup_times.len(), 2);
    }

    #[tokio::test]
    async fn test_health_score_calculation() {
        let provider_id = ProviderId::new();
        let collector = ProviderMetricsCollector::new(provider_id.clone());

        // Crear workers exitosos
        for _ in 0..10 {
            collector
                .record_worker_creation(Duration::from_secs(3), true)
                .await;
        }

        let health_score = collector.calculate_health_score_async().await;
        assert!(health_score > 0.9);
    }
}
