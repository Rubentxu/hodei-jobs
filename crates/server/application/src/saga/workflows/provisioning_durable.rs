//!
//! # Provisioning Workflow for saga-engine v4 (DurableWorkflow)
//!
//! This module provides the [`ProvisioningWorkflow`] implementation using the
//! [`DurableWorkflow`] trait for the Workflow-as-Code pattern.
//!
//! This is a migration from the legacy [`super::provisioning::ProvisioningWorkflow`]
//! which used the step-list pattern (`WorkflowDefinition`).
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::{ResourceRequirements, WorkerSpec};
use saga_engine_core::workflow::{
    Activity, ActivityOptions, DurableWorkflow, DurableWorkflowState, ExecuteActivityError,
    WorkflowConfig, WorkflowContext, WorkflowResult,
};

use crate::workers::provisioning::{MockProvisioningService, WorkerProvisioningService};

// =============================================================================
// Constants
// =============================================================================

/// Default configuration values for provisioning workflows
mod provisioning_config_defaults {
    use super::WorkflowConfig;

    /// Create the default configuration for provisioning workflows
    ///
    /// # Configuration Values
    /// - `default_timeout_secs`: 300 (5 minutes)
    /// - `max_step_retries`: 3
    /// - `retry_backoff_multiplier`: 2.0 (exponential backoff)
    /// - `retry_base_delay_ms`: 1000 (1 second)
    /// - `task_queue`: "provisioning"
    pub fn default_provisioning() -> WorkflowConfig {
        WorkflowConfig {
            default_timeout_secs: 300,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "provisioning".to_string(),
        }
    }
}

/// Helper function to get default provisioning configuration
fn default_provisioning_config() -> WorkflowConfig {
    provisioning_config_defaults::default_provisioning()
}

// =============================================================================
// Input/Output Types
// =============================================================================

/// Input for the Provisioning Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningInput {
    /// Worker specification to provision
    pub spec: WorkerSpecData,
    /// Provider ID to use for infrastructure creation
    pub provider_id: String,
    /// Job ID associated with this provisioning (optional)
    pub job_id: Option<String>,
}

impl ProvisioningInput {
    /// Create a new input with auto-generated idempotency key
    pub fn new(spec: WorkerSpecData, provider_id: String, job_id: Option<String>) -> Self {
        Self {
            spec,
            provider_id,
            job_id,
        }
    }

    /// Generate idempotency key for this input
    pub fn idempotency_key(&self) -> String {
        format!(
            "provisioning:{}:{}",
            self.provider_id,
            self.job_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string())
        )
    }
}

/// Worker specification data for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecData {
    pub image: String,
    pub resources: ResourceData,
    pub env: Vec<EnvVarData>,
    pub labels: Vec<String>,
    pub server_address: String,
    pub max_lifetime_seconds: u64,
}

impl WorkerSpecData {
    /// Create WorkerSpecData from domain WorkerSpec
    pub fn from_spec(spec: &WorkerSpec) -> Self {
        Self {
            image: spec.image.clone(),
            resources: ResourceData {
                cpu_cores: spec.resources.cpu_cores,
                memory_bytes: spec.resources.memory_bytes,
                disk_bytes: spec.resources.disk_bytes,
            },
            env: spec
                .environment
                .iter()
                .map(|(key, value)| EnvVarData {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
            labels: spec.labels.keys().cloned().collect(),
            server_address: spec.server_address.clone(),
            max_lifetime_seconds: spec.max_lifetime.as_secs(),
        }
    }

    /// Convert to domain WorkerSpec
    pub fn to_worker_spec(&self) -> WorkerSpec {
        let resources = ResourceRequirements {
            cpu_cores: self.resources.cpu_cores,
            memory_bytes: self.resources.memory_bytes,
            disk_bytes: self.resources.disk_bytes,
            gpu_count: 0,
            gpu_type: None,
        };

        let mut spec = WorkerSpec::new(self.image.clone(), self.server_address.clone())
            .with_resources(resources);

        for env in &self.env {
            spec = spec.with_env(&env.key, &env.value);
        }

        for label in &self.labels {
            spec = spec.with_label(label, "true");
        }

        spec
    }
}

/// Resource requirements data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceData {
    pub cpu_cores: f64,
    pub memory_bytes: i64,
    pub disk_bytes: i64,
}

/// Environment variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarData {
    pub key: String,
    pub value: String,
}

/// Output of the Provisioning Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningOutput {
    /// The provisioned worker ID
    pub worker_id: String,
    /// The job ID associated
    pub job_id: Option<String>,
    /// OTP token for worker registration
    pub otp_token: String,
    /// Provider used
    pub provider_id: String,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum ProvisioningError {
    #[error("Provider validation failed: {0}")]
    ProviderValidationFailed(String),

    #[error("Worker spec validation failed: {0}")]
    WorkerSpecValidationFailed(String),

    #[error("Worker provisioning failed: {0}")]
    WorkerProvisioningFailed(String),

    #[error("Activity execution failed: {0}")]
    ActivityFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities
// =============================================================================

/// Activity for validating provider availability
pub struct ValidateProviderActivity {
    /// Using underscore to indicate Debug is manually implemented
    _provisioning_service: Arc<dyn WorkerProvisioningService>,
}

impl std::fmt::Debug for ValidateProviderActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateProviderActivity")
            .field("_provisioning_service", &"<dyn WorkerProvisioningService>")
            .finish()
    }
}

impl ValidateProviderActivity {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            _provisioning_service: provisioning_service,
        }
    }
}

#[async_trait]
impl Activity for ValidateProviderActivity {
    const TYPE_ID: &'static str = "provisioning-validate-provider";

    type Input = ValidateProviderInput;
    type Output = ValidateProviderOutput;
    type Error = ProvisioningError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let provider_id = ProviderId::from_uuid(
            Uuid::parse_str(&input.provider_id)
                .map_err(|e| ProvisioningError::ProviderValidationFailed(e.to_string()))?,
        );

        let is_available = self
            ._provisioning_service
            .is_provider_available(&provider_id)
            .await
            .map_err(|e| ProvisioningError::ProviderValidationFailed(e.to_string()))?;

        if !is_available {
            return Err(ProvisioningError::ProviderValidationFailed(format!(
                "Provider {} is not available",
                input.provider_id
            )));
        }

        Ok(ValidateProviderOutput {
            provider_id: input.provider_id,
            is_available,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateProviderInput {
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateProviderOutput {
    pub provider_id: String,
    pub is_available: bool,
}

/// Activity for validating worker spec
pub struct ValidateWorkerSpecActivity {
    /// Using underscore to indicate Debug is manually implemented
    _provisioning_service: Arc<dyn WorkerProvisioningService>,
}

impl std::fmt::Debug for ValidateWorkerSpecActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateWorkerSpecActivity")
            .field("_provisioning_service", &"<dyn WorkerProvisioningService>")
            .finish()
    }
}

impl ValidateWorkerSpecActivity {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            _provisioning_service: provisioning_service,
        }
    }
}

#[async_trait]
impl Activity for ValidateWorkerSpecActivity {
    const TYPE_ID: &'static str = "provisioning-validate-spec";

    type Input = ValidateWorkerSpecInput;
    type Output = ValidateWorkerSpecOutput;
    type Error = ProvisioningError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let spec = input.spec.to_worker_spec();
        self._provisioning_service
            .validate_spec(&spec)
            .await
            .map_err(|e| ProvisioningError::WorkerSpecValidationFailed(e.to_string()))?;

        Ok(ValidateWorkerSpecOutput { is_valid: true })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateWorkerSpecInput {
    pub spec: WorkerSpecData,
    pub server_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateWorkerSpecOutput {
    pub is_valid: bool,
}

/// Activity for provisioning worker
pub struct ProvisionWorkerActivity {
    /// Using underscore to indicate Debug is manually implemented
    _provisioning_service: Arc<dyn WorkerProvisioningService>,
}

impl std::fmt::Debug for ProvisionWorkerActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionWorkerActivity")
            .field("_provisioning_service", &"<dyn WorkerProvisioningService>")
            .finish()
    }
}

impl ProvisionWorkerActivity {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            _provisioning_service: provisioning_service,
        }
    }
}

#[async_trait]
impl Activity for ProvisionWorkerActivity {
    const TYPE_ID: &'static str = "provisioning-provision-worker";

    type Input = ProvisionWorkerInput;
    type Output = ProvisionWorkerOutput;
    type Error = ProvisioningError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let provider_id = ProviderId::from_uuid(
            Uuid::parse_str(&input.provider_id)
                .map_err(|e| ProvisioningError::WorkerProvisioningFailed(e.to_string()))?,
        );

        let job_id = input
            .job_id
            .as_ref()
            .and_then(|j| JobId::from_string(j))
            .unwrap_or_else(JobId::new);

        let spec = input.spec.to_worker_spec();

        let result = self
            ._provisioning_service
            .provision_worker(&provider_id, spec, job_id.clone())
            .await
            .map_err(|e| ProvisioningError::WorkerProvisioningFailed(e.to_string()))?;

        Ok(ProvisionWorkerOutput {
            worker_id: result.worker_id.to_string(),
            otp_token: result.otp_token,
            provider_id: input.provider_id,
            job_id: Some(job_id.to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionWorkerInput {
    pub provider_id: String,
    pub spec: WorkerSpecData,
    pub server_address: String,
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionWorkerOutput {
    pub worker_id: String,
    pub otp_token: String,
    pub provider_id: String,
    pub job_id: Option<String>,
}

// =============================================================================
// Workflow
// =============================================================================

/// Provisioning Workflow using DurableWorkflow (Workflow-as-Code)
///
/// This workflow provisions a new worker by:
/// 1. Validating the provider is available
/// 2. Validating the worker spec is valid
/// 3. Provisioning the worker infrastructure
pub struct ProvisioningWorkflow {
    /// Activity for validating provider availability
    validate_provider_activity: ValidateProviderActivity,
    /// Activity for validating worker spec
    validate_spec_activity: ValidateWorkerSpecActivity,
    /// Activity for provisioning worker
    provision_worker_activity: ProvisionWorkerActivity,
    /// Workflow configuration
    config: WorkflowConfig,
}

impl Debug for ProvisioningWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisioningWorkflow")
            .field("config", &self.config)
            .finish()
    }
}

impl ProvisioningWorkflow {
    /// Create a new ProvisioningWorkflow with custom service and configuration
    ///
    /// This is the most flexible constructor, allowing full customization.
    ///
    /// # Arguments
    /// * `provisioning_service` - The service to use for worker provisioning
    /// * `config` - Custom workflow configuration
    ///
    /// # Example
    /// ```
    /// use std::sync::Arc;
    /// use hodei_server_application::saga::workflows::provisioning_durable::*;
    /// use hodei_server_application::workers::provisioning::WorkerProvisioningService;
    ///
    /// # fn example(service: Arc<dyn WorkerProvisioningService>) {
    /// let config = default_provisioning_config();
    /// let workflow = ProvisioningWorkflow::with_service_and_config(service, config);
    /// # }
    /// ```
    pub fn with_service_and_config(
        provisioning_service: Arc<dyn WorkerProvisioningService>,
        config: WorkflowConfig,
    ) -> Self {
        Self {
            validate_provider_activity: ValidateProviderActivity::new(provisioning_service.clone()),
            validate_spec_activity: ValidateWorkerSpecActivity::new(provisioning_service.clone()),
            provision_worker_activity: ProvisionWorkerActivity::new(provisioning_service),
            config,
        }
    }

    /// Create a new ProvisioningWorkflow with default configuration
    ///
    /// This is the recommended constructor for production use.
    ///
    /// # Arguments
    /// * `provisioning_service` - The service to use for worker provisioning
    ///
    /// # Example
    /// ```
    /// use std::sync::Arc;
    /// use hodei_server_application::saga::workflows::provisioning_durable::ProvisioningWorkflow;
    /// use hodei_server_application::workers::provisioning::WorkerProvisioningService;
    ///
    /// # fn example(service: Arc<dyn WorkerProvisioningService>) {
    /// let workflow = ProvisioningWorkflow::new(service);
    /// # }
    /// ```
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self::with_service_and_config(provisioning_service, default_provisioning_config())
    }

    /// Get the workflow configuration
    pub fn config(&self) -> &WorkflowConfig {
        &self.config
    }
}

/// Default implementation for ProvisioningWorkflow
///
/// # ⚠️ WARNING: FOR TESTING ONLY
///
/// This implementation creates a workflow with [MockProvisioningService],
/// which does **NOT** perform real provisioning operations.
///
/// **DO NOT USE IN PRODUCTION CODE.**
///
/// For production, use:
/// - [ProvisioningWorkflow::new()] - with real service
/// - [ProvisioningWorkflow::with_service_and_config()] - with custom config
///
/// # Example (Testing Only)
/// ```
/// use hodei_server_application::saga::workflows::provisioning_durable::ProvisioningWorkflow;
///
/// #[cfg(test)]
/// fn test_example() {
///     let workflow = ProvisioningWorkflow::default();
///     // Use in tests only
/// }
/// ```
impl Default for ProvisioningWorkflow {
    fn default() -> Self {
        let mock_service: Arc<dyn WorkerProvisioningService> =
            Arc::new(MockProvisioningService::new(vec![]));

        Self::with_service_and_config(mock_service, default_provisioning_config())
    }
}

#[async_trait]
impl DurableWorkflow for ProvisioningWorkflow {
    const TYPE_ID: &'static str = "provisioning";
    const VERSION: u32 = 2; // Version 2 = migrated to DurableWorkflow

    type Input = ProvisioningInput;
    type Output = ProvisioningOutput;
    type Error = ProvisioningError;

    /// Execute the provisioning workflow
    ///
    /// This method implements the workflow logic using the Workflow-as-Code pattern.
    /// Each activity call can pause the workflow and resume when complete.
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        tracing::info!(
            execution_id = %context.execution_id.0,
            provider_id = %input.provider_id,
            "Starting provisioning workflow"
        );

        // Step 1: Validate provider
        let validate_input = ValidateProviderInput {
            provider_id: input.provider_id.clone(),
        };

        let validate_result = context
            .execute_activity(&self.validate_provider_activity, validate_input.clone())
            .await;

        match validate_result {
            Ok(output) => {
                tracing::info!(provider_id = %output.provider_id, "Provider validated");
            }
            Err(e) if e.is_failed() => {
                return Err(ProvisioningError::ProviderValidationFailed(
                    e.as_failed()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }
            Err(e) if e.is_paused() => {
                // Workflow paused, will resume when activity completes
                return Err(ProvisioningError::ActivityFailed(
                    "Workflow paused waiting for provider validation".to_string(),
                ));
            }
            _ => {}
        }

        // Step 2: Validate worker spec
        let spec_input = ValidateWorkerSpecInput {
            spec: input.spec.clone(),
            server_address: input.spec.server_address.clone(),
        };

        let spec_result = context
            .execute_activity(&self.validate_spec_activity, spec_input.clone())
            .await;

        match spec_result {
            Ok(output) => {
                tracing::info!(is_valid = %output.is_valid, "Worker spec validated");
            }
            Err(e) if e.is_failed() => {
                return Err(ProvisioningError::WorkerSpecValidationFailed(
                    e.as_failed()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }
            Err(e) if e.is_paused() => {
                return Err(ProvisioningError::ActivityFailed(
                    "Workflow paused waiting for spec validation".to_string(),
                ));
            }
            _ => {}
        }

        // Step 3: Provision worker
        let provision_input = ProvisionWorkerInput {
            provider_id: input.provider_id.clone(),
            spec: input.spec.clone(),
            server_address: input.spec.server_address.clone(),
            job_id: input.job_id.clone(),
        };

        let provision_result = context
            .execute_activity(&self.provision_worker_activity, provision_input.clone())
            .await;

        match provision_result {
            Ok(output) => {
                tracing::info!(
                    worker_id = %output.worker_id,
                    "Worker provisioned successfully"
                );

                // Return the final output
                return Ok(ProvisioningOutput {
                    worker_id: output.worker_id,
                    job_id: output.job_id,
                    otp_token: output.otp_token,
                    provider_id: output.provider_id,
                });
            }
            Err(e) if e.is_failed() => {
                return Err(ProvisioningError::WorkerProvisioningFailed(
                    e.as_failed()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }
            Err(e) if e.is_paused() => {
                return Err(ProvisioningError::ActivityFailed(
                    "Workflow paused waiting for worker provisioning".to_string(),
                ));
            }
            _ => {}
        }

        // Should not reach here
        Err(ProvisioningError::ActivityFailed(
            "Unexpected workflow state".to_string(),
        ))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::provisioning::{MockWorkerProvisioning, MockWorkerRegistry};
    use saga_engine_core::workflow::WorkflowContext;
    use std::sync::Arc;

    #[test]
    fn test_provisioning_input_idempotency_key() {
        let input = ProvisioningInput::new(
            WorkerSpecData {
                image: "worker:latest".to_string(),
                resources: ResourceData {
                    cpu_cores: 2.0,
                    memory_bytes: 4096 * 1024 * 1024,
                    disk_bytes: 10240 * 1024 * 1024,
                },
                env: vec![EnvVarData {
                    key: "KEY".to_string(),
                    value: "VALUE".to_string(),
                }],
                labels: vec!["label1".to_string()],
                server_address: "http://localhost:50051".to_string(),
                max_lifetime_seconds: 3600,
            },
            "provider-1".to_string(),
            Some("job-123".to_string()),
        );

        let key = input.idempotency_key();
        assert!(key.starts_with("provisioning:provider-1:job-123"));
    }

    #[test]
    fn test_provisioning_workflow_type_id() {
        assert_eq!(ProvisioningWorkflow::TYPE_ID, "provisioning");
        assert_eq!(ProvisioningWorkflow::VERSION, 2);
    }

    #[test]
    fn test_worker_spec_data_to_spec() {
        let data = WorkerSpecData {
            image: "worker:latest".to_string(),
            resources: ResourceData {
                cpu_cores: 2.0,
                memory_bytes: 4096 * 1024 * 1024,
                disk_bytes: 10240 * 1024 * 1024,
            },
            env: vec![EnvVarData {
                key: "KEY".to_string(),
                value: "VALUE".to_string(),
            }],
            labels: vec!["label1".to_string()],
            server_address: "http://localhost:50051".to_string(),
            max_lifetime_seconds: 3600,
        };

        let spec = data.to_worker_spec();
        assert_eq!(spec.image, "worker:latest");
        assert_eq!(spec.resources.cpu_cores, 2.0);
        assert!(spec.environment.contains_key("KEY"));
        assert!(spec.labels.contains_key("label1"));
    }

    #[test]
    fn test_default_creates_workflow_with_mock_service() {
        let workflow = ProvisioningWorkflow::default();

        // Verify default configuration is applied
        assert_eq!(workflow.config().task_queue, "provisioning");
        assert_eq!(workflow.config().default_timeout_secs, 300);
        assert_eq!(workflow.config().max_step_retries, 3);
    }

    #[test]
    fn test_with_service_and_config_uses_custom_config() {
        let mock_service: Arc<dyn WorkerProvisioningService> =
            Arc::new(MockProvisioningService::new(vec![]));

        let custom_config = WorkflowConfig {
            default_timeout_secs: 600,
            max_step_retries: 5,
            retry_backoff_multiplier: 3.0,
            retry_base_delay_ms: 2000,
            task_queue: "custom-queue".to_string(),
        };

        let workflow =
            ProvisioningWorkflow::with_service_and_config(mock_service, custom_config.clone());

        assert_eq!(workflow.config().default_timeout_secs, 600);
        assert_eq!(workflow.config().max_step_retries, 5);
        assert_eq!(workflow.config().task_queue, "custom-queue");
    }

    #[test]
    fn test_workflow_config_default_provisioning() {
        let config = provisioning_config_defaults::default_provisioning();

        assert_eq!(config.default_timeout_secs, 300);
        assert_eq!(config.max_step_retries, 3);
        assert_eq!(config.retry_backoff_multiplier, 2.0);
        assert_eq!(config.retry_base_delay_ms, 1000);
        assert_eq!(config.task_queue, "provisioning");
    }

    #[test]
    fn test_provisioning_workflow_creation() {
        let provisioning: Arc<dyn WorkerProvisioningService> =
            Arc::new(MockProvisioningService::new(vec![]));
        let workflow = ProvisioningWorkflow::new(provisioning);

        assert_eq!(workflow.config().task_queue, "provisioning");
        assert_eq!(workflow.config().default_timeout_secs, 300);
    }
}
