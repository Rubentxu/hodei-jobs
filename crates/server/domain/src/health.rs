//! Health Check Domain Types and Service
//!
//! Core health check types and service that can be used across all layers
//! without creating circular dependencies.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::watch;

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub check_timeout: Duration,
    pub include_details: bool,
    pub check_interval: Duration,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_timeout: Duration::from_secs(30),
            include_details: true,
            check_interval: Duration::from_secs(10),
        }
    }
}

impl HealthCheckConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_check_timeout(mut self, timeout: Duration) -> Self {
        self.check_timeout = timeout;
        self
    }

    pub fn with_include_details(mut self, include: bool) -> Self {
        self.include_details = include;
        self
    }

    pub fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }
}

/// Health check error types
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum HealthCheckError {
    #[error("Database connection failed: {message}")]
    DatabaseError { message: String },

    #[error("NATs connection failed: {message}")]
    NatsError { message: String },

    #[error("Provider health check failed: {provider_id} - {message}")]
    ProviderError {
        provider_id: String,
        message: String,
    },

    #[error("Saga orchestrator check failed: {message}")]
    SagaError { message: String },

    #[error("Health check timed out after {timeout:?}")]
    Timeout { timeout: Duration },

    #[error("Component not initialized: {component}")]
    NotInitialized { component: String },
}

/// Overall health status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// Component health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Detailed health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub status: HealthStatus,
    pub timestamp: DateTime<Utc>,
    pub version: String,
    pub uptime_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<ComponentHealth>>,
    pub check_duration_ms: u64,
}

/// Liveness check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub uptime_seconds: u64,
}

/// Readiness check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub status: HealthStatus,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<ComponentHealth>>,
    pub check_duration_ms: u64,
}

/// Saga health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaHealthResponse {
    pub status: HealthStatus,
    pub timestamp: DateTime<Utc>,
    pub active_sagas: u64,
    pub error_sagas: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_status: Option<ComponentHealth>,
    pub check_duration_ms: u64,
}

/// Trait for checking component health
#[async_trait::async_trait]
pub trait HealthChecker: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self) -> Result<ComponentHealth, HealthCheckError>;
}

/// Saga health checker trait
#[async_trait::async_trait]
pub trait SagaHealthChecker: Send + Sync {
    async fn check_orchestrator(&self) -> Result<ComponentHealth, HealthCheckError>;
    async fn get_active_saga_count(&self) -> Result<u64, HealthCheckError>;
    async fn get_error_saga_count(&self) -> Result<u64, HealthCheckError>;
}

/// Main health check service (domain implementation)
#[derive(Clone)]
pub struct HealthCheckService {
    config: HealthCheckConfig,
    checkers: Vec<Arc<dyn HealthChecker>>,
    process_start: DateTime<Utc>,
    version: String,
    saga_checker: Option<Arc<dyn SagaHealthChecker>>,
    state_tx: watch::Sender<HealthStatus>,
}

impl HealthCheckService {
    pub fn new(
        config: HealthCheckConfig,
        checkers: Vec<Arc<dyn HealthChecker>>,
        version: String,
    ) -> Self {
        let (state_tx, _) = watch::channel(HealthStatus::Unknown);
        Self {
            config,
            checkers,
            process_start: Utc::now(),
            version,
            saga_checker: None,
            state_tx,
        }
    }

    pub fn with_saga_checker(mut self, checker: Arc<dyn SagaHealthChecker>) -> Self {
        self.saga_checker = Some(checker);
        self
    }

    pub fn register_checker(&mut self, checker: Arc<dyn HealthChecker>) {
        self.checkers.push(checker);
    }

    fn uptime(&self) -> u64 {
        let now = Utc::now();
        let duration = now.signed_duration_since(self.process_start);
        duration.to_std().unwrap_or_default().as_secs()
    }

    pub async fn check_liveness(&self) -> LivenessResponse {
        LivenessResponse {
            status: "ok".to_string(),
            timestamp: Utc::now(),
            uptime_seconds: self.uptime(),
        }
    }

    pub async fn check_readiness(&self) -> ReadinessResponse {
        let start = Utc::now();
        let mut overall_status = HealthStatus::Healthy;
        let mut components = Vec::new();

        for checker in &self.checkers {
            match checker.check().await {
                Ok(component) => {
                    if !component.status.is_healthy() && overall_status == HealthStatus::Healthy {
                        overall_status = component.status.clone();
                    }
                    components.push(component);
                }
                Err(e) => {
                    overall_status = HealthStatus::Unhealthy;
                    components.push(ComponentHealth {
                        name: checker.name().to_string(),
                        status: HealthStatus::Unhealthy,
                        last_check: Some(Utc::now()),
                        details: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let duration = Utc::now().signed_duration_since(start);
        let check_duration_ms = duration.to_std().unwrap_or_default().as_millis() as u64;

        let _ = self.state_tx.send(overall_status.clone());

        ReadinessResponse {
            ready: overall_status.is_healthy(),
            status: overall_status,
            timestamp: Utc::now(),
            components: Some(components),
            check_duration_ms,
        }
    }

    pub async fn check_saga_health(&self) -> SagaHealthResponse {
        let start = Utc::now();
        let mut status = HealthStatus::Healthy;
        let mut orchestrator_status = None;

        if let Some(ref saga_checker) = self.saga_checker {
            match saga_checker.check_orchestrator().await {
                Ok(orch) => {
                    if !orch.status.is_healthy() {
                        status = orch.status.clone();
                    }
                    orchestrator_status = Some(orch);
                }
                Err(e) => {
                    status = HealthStatus::Unhealthy;
                    orchestrator_status = Some(ComponentHealth {
                        name: "saga_orchestrator".to_string(),
                        status: HealthStatus::Unhealthy,
                        last_check: Some(Utc::now()),
                        details: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let active_sagas = if let Some(ref saga_checker) = self.saga_checker {
            saga_checker.get_active_saga_count().await.unwrap_or(0)
        } else {
            0
        };

        let error_sagas = if let Some(ref saga_checker) = self.saga_checker {
            saga_checker.get_error_saga_count().await.unwrap_or(0)
        } else {
            0
        };

        if error_sagas > 10 {
            status = HealthStatus::Degraded;
        }

        let duration = Utc::now().signed_duration_since(start);
        let check_duration_ms = duration.to_std().unwrap_or_default().as_millis() as u64;

        SagaHealthResponse {
            status,
            timestamp: Utc::now(),
            active_sagas,
            error_sagas,
            orchestrator_status,
            check_duration_ms,
        }
    }

    pub async fn check_full(&self) -> HealthCheckResponse {
        let start = Utc::now();
        let readiness = self.check_readiness().await;
        let saga = self.check_saga_health().await;

        let overall_status =
            if readiness.status == HealthStatus::Healthy && saga.status == HealthStatus::Healthy {
                HealthStatus::Healthy
            } else if readiness.status == HealthStatus::Unhealthy
                || saga.status == HealthStatus::Unhealthy
            {
                HealthStatus::Unhealthy
            } else {
                HealthStatus::Degraded
            };

        let duration = Utc::now().signed_duration_since(start);
        let check_duration_ms = duration.to_std().unwrap_or_default().as_millis() as u64;

        let mut all_components = Vec::new();
        if let Some(ref components) = readiness.components {
            all_components.extend(components.clone());
        }
        if let Some(ref orchestrator) = saga.orchestrator_status {
            all_components.push(orchestrator.clone());
        }

        HealthCheckResponse {
            status: overall_status,
            timestamp: Utc::now(),
            version: self.version.clone(),
            uptime_seconds: self.uptime(),
            components: Some(all_components),
            check_duration_ms,
        }
    }

    pub fn current_state(&self) -> HealthStatus {
        self.state_tx.borrow().clone()
    }
}

/// Composite health checker
#[derive(Clone)]
pub struct CompositeHealthChecker {
    name: String,
    checkers: Vec<Arc<dyn HealthChecker>>,
}

impl CompositeHealthChecker {
    pub fn new(name: String) -> Self {
        Self {
            name,
            checkers: Vec::new(),
        }
    }

    pub fn with_checker(mut self, checker: Arc<dyn HealthChecker>) -> Self {
        self.checkers.push(checker);
        self
    }
}

#[async_trait]
impl HealthChecker for CompositeHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> Result<ComponentHealth, HealthCheckError> {
        let mut all_healthy = true;
        let mut error_messages = Vec::new();
        let mut details = HashMap::new();
        let mut component_count = 0;

        for checker in &self.checkers {
            component_count += 1;
            match checker.check().await {
                Ok(component) => {
                    if !component.status.is_healthy() {
                        all_healthy = false;
                        if let Some(ref err) = component.error {
                            error_messages.push(format!("{}: {}", component.name, err));
                        }
                    }
                    details.insert(
                        format!("{}_status", component.name),
                        format!("{:?}", component.status),
                    );
                }
                Err(e) => {
                    all_healthy = false;
                    error_messages.push(format!("{}: {}", checker.name(), e));
                }
            }
        }

        if all_healthy {
            Ok(ComponentHealth {
                name: self.name.clone(),
                status: HealthStatus::Healthy,
                last_check: Some(Utc::now()),
                details: Some(details),
                error: None,
            })
        } else {
            Ok(ComponentHealth {
                name: self.name.clone(),
                status: if component_count > 0 {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Unhealthy
                },
                last_check: Some(Utc::now()),
                details: Some(details),
                error: Some(error_messages.join("; ")),
            })
        }
    }
}

use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::time::Duration;

    struct MockHealthChecker {
        name: String,
        healthy: bool,
    }

    #[async_trait]
    impl HealthChecker for MockHealthChecker {
        fn name(&self) -> &str {
            &self.name
        }

        async fn check(&self) -> Result<ComponentHealth, HealthCheckError> {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(ComponentHealth {
                name: self.name.clone(),
                status: if self.healthy {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy
                },
                last_check: Some(Utc::now()),
                details: None,
                error: if self.healthy {
                    None
                } else {
                    Some("mock error".to_string())
                },
            })
        }
    }

    #[tokio::test]
    async fn test_liveness_check() {
        let config = HealthCheckConfig::new();
        let service = HealthCheckService::new(config, Vec::new(), "test-0.1.0".to_string());
        let response = service.check_liveness().await;
        assert_eq!(response.status, "ok");
    }

    #[tokio::test]
    async fn test_readiness_check() {
        let checker: Arc<dyn HealthChecker> =
            Arc::new(MockHealthChecker::new("test".to_string(), true));
        let config = HealthCheckConfig::new();
        let service = HealthCheckService::new(config, vec![checker], "test-0.1.0".to_string());
        let response = service.check_readiness().await;
        assert!(response.ready);
        assert_eq!(response.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_composite_checker() {
        let checker1: Arc<dyn HealthChecker> =
            Arc::new(MockHealthChecker::new("comp1".to_string(), true));
        let checker2: Arc<dyn HealthChecker> =
            Arc::new(MockHealthChecker::new("comp2".to_string(), false));

        let composite: Arc<dyn HealthChecker> = Arc::new(
            CompositeHealthChecker::new("composite".to_string())
                .with_checker(checker1)
                .with_checker(checker2),
        );

        let result = composite.check().await.unwrap();
        assert_eq!(result.status, HealthStatus::Degraded);
    }
}
