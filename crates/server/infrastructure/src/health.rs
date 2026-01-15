//! Production-ready Health Check Implementations
//!
//! Real implementations that verify connectivity to infrastructure components.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hodei_server_domain::health::{
    ComponentHealth, HealthCheckConfig, HealthCheckError, HealthChecker, HealthStatus,
};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::error;

/// PostgreSQL Database Health Checker
#[derive(Clone)]
pub struct DatabaseHealthChecker {
    pool: Arc<Pool<Postgres>>,
    database_name: String,
}

impl DatabaseHealthChecker {
    pub fn new(pool: Arc<Pool<Postgres>>, database_name: String) -> Self {
        Self {
            pool,
            database_name,
        }
    }
}

#[async_trait]
impl HealthChecker for DatabaseHealthChecker {
    fn name(&self) -> &str {
        "database"
    }

    async fn check(&self) -> Result<ComponentHealth, HealthCheckError> {
        let start = Utc::now();

        let result = sqlx::query("SELECT 1").fetch_one(&*self.pool).await;

        match result {
            Ok(_) => Ok(ComponentHealth {
                name: "database".to_string(),
                status: HealthStatus::Healthy,
                last_check: Some(start),
                details: Some(
                    [
                        ("database".to_string(), self.database_name.clone()),
                        ("connection".to_string(), "active".to_string()),
                    ]
                    .into(),
                ),
                error: None,
            }),
            Err(e) => {
                error!(error = %e, "Database health check failed");
                Ok(ComponentHealth {
                    name: "database".to_string(),
                    status: HealthStatus::Unhealthy,
                    last_check: Some(start),
                    details: None,
                    error: Some(format!("Database connection failed: {}", e)),
                })
            }
        }
    }
}

/// NATS Connection Health Checker
#[derive(Clone)]
pub struct NatsHealthChecker {
    client: Arc<async_nats::Client>,
    connection_name: String,
}

impl NatsHealthChecker {
    pub fn new(client: Arc<async_nats::Client>, connection_name: String) -> Self {
        Self {
            client,
            connection_name,
        }
    }
}

#[async_trait]
impl HealthChecker for NatsHealthChecker {
    fn name(&self) -> &str {
        "nats"
    }

    async fn check(&self) -> Result<ComponentHealth, HealthCheckError> {
        let start = Utc::now();

        if let Err(e) = self
            .client
            .publish("_hodei.health.ping".to_string(), bytes::Bytes::new())
            .await
        {
            return Ok(ComponentHealth {
                name: "nats".to_string(),
                status: HealthStatus::Unhealthy,
                last_check: Some(start),
                details: None,
                error: Some(format!("Publish failed: {}", e)),
            });
        }

        if let Err(e) = self.client.flush().await {
            return Ok(ComponentHealth {
                name: "nats".to_string(),
                status: HealthStatus::Unhealthy,
                last_check: Some(start),
                details: None,
                error: Some(format!("Flush failed: {}", e)),
            });
        }

        let mut details = HashMap::new();
        details.insert("connection".to_string(), self.connection_name.clone());
        details.insert("status".to_string(), "connected".to_string());

        Ok(ComponentHealth {
            name: "nats".to_string(),
            status: HealthStatus::Healthy,
            last_check: Some(start),
            details: Some(details),
            error: None,
        })
    }
}

/// Provider Health Summary
#[derive(Debug, Clone, Default)]
pub struct ProviderHealthSummary {
    pub total: usize,
    pub healthy: usize,
    pub unhealthy: usize,
    pub unknown: usize,
    pub providers: HashMap<String, HealthStatus>,
}

#[derive(Debug, Clone)]
struct ProviderHealthRecord {
    status: HealthStatus,
    details: Option<HashMap<String, String>>,
    error: Option<String>,
    last_update: DateTime<Utc>,
}

/// Provider Health Registry (thread-safe using Tokio RwLock)
#[derive(Default, Clone)]
pub struct ProviderHealthRegistry {
    records: Arc<RwLock<HashMap<String, ProviderHealthRecord>>>,
}

impl ProviderHealthRegistry {
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(
        &self,
        provider_id: String,
        status: HealthStatus,
        details: Option<HashMap<String, String>>,
        error: Option<String>,
    ) {
        let mut records = self.records.write().await;
        records.insert(
            provider_id,
            ProviderHealthRecord {
                status,
                details,
                error,
                last_update: Utc::now(),
            },
        );
    }

    pub async fn get(&self, provider_id: &str) -> Option<ProviderHealthRecord> {
        let records = self.records.read().await;
        records.get(provider_id).cloned()
    }

    pub async fn get_summary(&self) -> ProviderHealthSummary {
        let records = self.records.read().await;
        let mut summary = ProviderHealthSummary::default();

        for (provider_id, record) in records.iter() {
            summary.total += 1;
            match record.status {
                HealthStatus::Healthy => summary.healthy += 1,
                HealthStatus::Unhealthy => summary.unhealthy += 1,
                HealthStatus::Degraded => summary.healthy += 1,
                HealthStatus::Unknown => summary.unknown += 1,
            }
            let status_for_insert = record.status.clone();
            summary
                .providers
                .insert(provider_id.clone(), status_for_insert);
        }

        summary
    }
}

/// Provider Health Checker (Composite)
#[derive(Clone)]
pub struct ProviderHealthChecker {
    health_registry: Arc<ProviderHealthRegistry>,
    timeout: Duration,
}

impl ProviderHealthChecker {
    pub fn new(health_registry: Arc<ProviderHealthRegistry>) -> Self {
        Self {
            health_registry,
            timeout: Duration::from_secs(30),
        }
    }

    pub async fn update_provider_status(
        &self,
        provider_id: String,
        status: HealthStatus,
        details: Option<HashMap<String, String>>,
        error: Option<String>,
    ) {
        self.health_registry
            .insert(provider_id, status, details, error)
            .await;
    }

    pub async fn get_summary(&self) -> ProviderHealthSummary {
        self.health_registry.get_summary().await
    }
}

#[async_trait]
impl HealthChecker for ProviderHealthChecker {
    fn name(&self) -> &str {
        "providers"
    }

    async fn check(&self) -> Result<ComponentHealth, HealthCheckError> {
        let start = Utc::now();
        let summary = self.get_summary().await;

        let status = if summary.unhealthy > 0 {
            HealthStatus::Unhealthy
        } else if summary.unknown > 0 && summary.healthy == 0 {
            HealthStatus::Unknown
        } else if summary.unhealthy == 0 && summary.unknown > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        let mut details = HashMap::new();
        details.insert("total_providers".to_string(), summary.total.to_string());
        details.insert("healthy".to_string(), summary.healthy.to_string());
        details.insert("unhealthy".to_string(), summary.unhealthy.to_string());

        let error = if summary.unhealthy > 0 {
            Some(format!("{} provider(s) unhealthy", summary.unhealthy))
        } else {
            None
        };

        Ok(ComponentHealth {
            name: "providers".to_string(),
            status,
            last_check: Some(start),
            details: Some(details),
            error,
        })
    }
}

/// Health Check Service Builder
#[derive(Default, Clone)]
pub struct HealthCheckServiceBuilder {
    config: HealthCheckConfig,
    checkers: Vec<Arc<dyn HealthChecker>>,
    version: String,
}

impl HealthCheckServiceBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(mut self, config: HealthCheckConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    pub fn with_database_checker(self, pool: Arc<Pool<Postgres>>, database_name: &str) -> Self {
        let checker: Arc<dyn HealthChecker> =
            Arc::new(DatabaseHealthChecker::new(pool, database_name.to_string()));
        self.with_checker(checker)
    }

    pub fn with_nats_checker(self, client: Arc<async_nats::Client>, connection_name: &str) -> Self {
        let checker: Arc<dyn HealthChecker> =
            Arc::new(NatsHealthChecker::new(client, connection_name.to_string()));
        self.with_checker(checker)
    }

    pub fn with_provider_checker(self, health_registry: Arc<ProviderHealthRegistry>) -> Self {
        let checker: Arc<dyn HealthChecker> = Arc::new(ProviderHealthChecker::new(health_registry));
        self.with_checker(checker)
    }

    pub fn with_checker(mut self, checker: Arc<dyn HealthChecker>) -> Self {
        self.checkers.push(checker);
        self
    }

    pub fn build(self) -> hodei_server_domain::health::HealthCheckService {
        hodei_server_domain::health::HealthCheckService::new(
            self.config,
            self.checkers,
            self.version,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::time::Duration;

    struct MockHealthyChecker;

    #[async_trait]
    impl HealthChecker for MockHealthyChecker {
        fn name(&self) -> &str {
            "healthy"
        }

        async fn check(&self) -> Result<ComponentHealth, HealthCheckError> {
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(ComponentHealth {
                name: "healthy".to_string(),
                status: HealthStatus::Healthy,
                last_check: Some(Utc::now()),
                details: None,
                error: None,
            })
        }
    }

    #[tokio::test]
    async fn test_provider_health_registry() {
        let registry = ProviderHealthRegistry::new();

        registry
            .insert(
                "docker-1".to_string(),
                HealthStatus::Healthy,
                Some([("workers".to_string(), "5".to_string())].into()),
                None,
            )
            .await;

        let record = registry.get("docker-1").await;
        assert!(record.is_some());
        assert_eq!(record.unwrap().status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_provider_health_summary() {
        let registry = Arc::new(ProviderHealthRegistry::new());
        let checker = ProviderHealthChecker::new(registry);

        checker
            .update_provider_status("docker-1".to_string(), HealthStatus::Healthy, None, None)
            .await;

        checker
            .update_provider_status(
                "k8s-1".to_string(),
                HealthStatus::Unhealthy,
                None,
                Some("Connection timeout".to_string()),
            )
            .await;

        let summary = checker.get_summary().await;
        assert_eq!(summary.total, 2);
        assert_eq!(summary.healthy, 1);
        assert_eq!(summary.unhealthy, 1);
    }

    #[tokio::test]
    async fn test_service_builder() {
        let builder = HealthCheckServiceBuilder::new()
            .with_version("1.0.0")
            .with_checker(Arc::new(MockHealthyChecker));

        let service = builder.build();
        assert_eq!(service.check_liveness().await.status, "ok");
    }
}
