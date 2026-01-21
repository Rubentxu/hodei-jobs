//!
//! # Provisioning Workflow for saga-engine v4
//!
//! This module provides the ProvisioningWorkflow implementation for saga-engine v4.0,
//! migrating from the legacy ProvisioningSaga to the new Event Sourcing based workflow.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use hodei_server_domain::shared_kernel::{JobId, ProviderId};
use hodei_server_domain::workers::{ResourceRequirements, WorkerSpec};
use saga_engine_core::workflow::{
    DynWorkflowStep, StepCompensationError, StepError, StepErrorKind, StepResult, WorkflowConfig,
    WorkflowContext, WorkflowDefinition, WorkflowExecutor, WorkflowStep,
};

use crate::workers::provisioning::WorkerProvisioningService;

// =============================================================================
// Input/Output Types
// =============================================================================

/// Input for the Provisioning Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningWorkflowInput {
    /// Worker specification to provision
    pub spec: WorkerSpecData,
    /// Provider ID to use for infrastructure creation
    pub provider_id: String,
    /// Job ID associated with this provisioning (optional)
    pub job_id: Option<String>,
    /// Idempotency key for deduplication
    pub idempotency_key: Option<String>,
}

impl ProvisioningWorkflowInput {
    /// Create a new input with auto-generated idempotency key
    pub fn new(spec: WorkerSpecData, provider_id: String, job_id: Option<String>) -> Self {
        let idempotency_key = Some(format!(
            "provisioning:{}:{}",
            provider_id,
            job_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string())
        ));
        Self {
            spec,
            provider_id,
            job_id,
            idempotency_key,
        }
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

impl From<&WorkerSpec> for WorkerSpecData {
    fn from(spec: &WorkerSpec) -> Self {
        Self {
            image: spec.image.clone(),
            resources: ResourceData::from(&spec.resources),
            env: spec
                .environment
                .iter()
                .map(|(k, v)| EnvVarData {
                    key: k.clone(),
                    value: v.clone(),
                })
                .collect(),
            labels: spec.labels.keys().cloned().collect(),
            server_address: spec.server_address.clone(),
            max_lifetime_seconds: spec.max_lifetime.as_secs(),
        }
    }
}

/// Resource requirements data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceData {
    pub cpu_cores: f64,
    pub memory_bytes: i64,
    pub disk_bytes: i64,
}

impl From<&ResourceRequirements> for ResourceData {
    fn from(r: &ResourceRequirements) -> Self {
        Self {
            cpu_cores: r.cpu_cores,
            memory_bytes: r.memory_bytes,
            disk_bytes: r.disk_bytes,
        }
    }
}

/// Environment variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarData {
    pub key: String,
    pub value: String,
}

/// Output of the Provisioning Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningWorkflowOutput {
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
pub enum ProvisioningWorkflowError {
    #[error("Provider validation failed: {0}")]
    ProviderValidationFailed(String),

    #[error("Worker provisioning failed: {0}")]
    WorkerProvisioningFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities
// =============================================================================

/// Activity for validating provider availability
pub struct ValidateProviderActivity {
    provisioning_service: Arc<dyn WorkerProvisioningService>,
}

impl std::fmt::Debug for ValidateProviderActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateProviderActivity")
            .field("provisioning_service", &"<dyn WorkerProvisioningService>")
            .finish()
    }
}

impl ValidateProviderActivity {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            provisioning_service,
        }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for ValidateProviderActivity {
    const TYPE_ID: &'static str = "provisioning-validate-provider";

    type Input = ValidateProviderInput;
    type Output = ValidateProviderOutput;
    type Error = ProvisioningWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let provider_id = ProviderId::from_uuid(
            Uuid::parse_str(&input.provider_id)
                .map_err(|e| ProvisioningWorkflowError::ProviderValidationFailed(e.to_string()))?,
        );

        // Check if provider is available
        let is_available = self
            .provisioning_service
            .is_provider_available(&provider_id)
            .await
            .map_err(|e| ProvisioningWorkflowError::ProviderValidationFailed(e.to_string()))?;

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
    provisioning_service: Arc<dyn WorkerProvisioningService>,
}

impl std::fmt::Debug for ValidateWorkerSpecActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateWorkerSpecActivity")
            .field("provisioning_service", &"<dyn WorkerProvisioningService>")
            .finish()
    }
}

impl ValidateWorkerSpecActivity {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            provisioning_service,
        }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for ValidateWorkerSpecActivity {
    const TYPE_ID: &'static str = "provisioning-validate-spec";

    type Input = ValidateWorkerSpecInput;
    type Output = ValidateWorkerSpecOutput;
    type Error = ProvisioningWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let spec = input.to_worker_spec();
        self.provisioning_service
            .validate_spec(&spec)
            .await
            .map_err(|e| ProvisioningWorkflowError::ProviderValidationFailed(e.to_string()))?;

        Ok(ValidateWorkerSpecOutput { is_valid: true })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateWorkerSpecInput {
    pub spec: WorkerSpecData,
    pub server_address: String,
}

impl ValidateWorkerSpecInput {
    pub fn to_worker_spec(&self) -> WorkerSpec {
        let resources = ResourceRequirements {
            cpu_cores: self.spec.resources.cpu_cores,
            memory_bytes: self.spec.resources.memory_bytes,
            disk_bytes: self.spec.resources.disk_bytes,
            gpu_count: 0,
            gpu_type: None,
        };

        WorkerSpec::new(self.spec.image.clone(), self.server_address.clone())
            .with_resources(resources)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateWorkerSpecOutput {
    pub is_valid: bool,
}

/// Activity for provisioning worker
pub struct ProvisionWorkerActivity {
    provisioning_service: Arc<dyn WorkerProvisioningService>,
}

impl std::fmt::Debug for ProvisionWorkerActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionWorkerActivity")
            .field("provisioning_service", &"<dyn WorkerProvisioningService>")
            .finish()
    }
}

impl ProvisionWorkerActivity {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            provisioning_service,
        }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for ProvisionWorkerActivity {
    const TYPE_ID: &'static str = "provisioning-provision-worker";

    type Input = ProvisionWorkerInput;
    type Output = ProvisionWorkerOutput;
    type Error = ProvisioningWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let provider_id = ProviderId::from_uuid(
            Uuid::parse_str(&input.provider_id)
                .map_err(|e| ProvisioningWorkflowError::ProviderValidationFailed(e.to_string()))?,
        );
        let job_id = JobId::new();
        let spec = input.to_worker_spec();

        let result = self
            .provisioning_service
            .provision_worker(&provider_id, spec, job_id.clone())
            .await
            .map_err(|e| ProvisioningWorkflowError::WorkerProvisioningFailed(e.to_string()))?;

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

impl ProvisionWorkerInput {
    pub fn to_worker_spec(&self) -> WorkerSpec {
        let resources = ResourceRequirements {
            cpu_cores: self.spec.resources.cpu_cores,
            memory_bytes: self.spec.resources.memory_bytes,
            disk_bytes: self.spec.resources.disk_bytes,
            gpu_count: 0,
            gpu_type: None,
        };

        let mut spec = WorkerSpec::new(self.spec.image.clone(), self.server_address.clone())
            .with_resources(resources);

        for env in &self.spec.env {
            spec = spec.with_env(&env.key, &env.value);
        }

        for label in &self.spec.labels {
            spec = spec.with_label(label, "true");
        }

        spec
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionWorkerOutput {
    pub worker_id: String,
    pub otp_token: String,
    pub provider_id: String,
    pub job_id: Option<String>,
}

// =============================================================================
// Workflow Steps
// =============================================================================

/// Step for validating provider availability
pub struct ValidateProviderStep {
    activity: ValidateProviderActivity,
}

impl std::fmt::Debug for ValidateProviderStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateProviderStep")
            .field("activity", &self.activity)
            .finish()
    }
}

impl ValidateProviderStep {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            activity: ValidateProviderActivity::new(provisioning_service),
        }
    }
}

#[async_trait]
impl WorkflowStep for ValidateProviderStep {
    const TYPE_ID: &'static str = "provisioning-validate-provider";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: ProvisioningWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = ValidateProviderInput {
            provider_id: input.provider_id.clone(),
        };

        match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input).await {
            Ok(output) => {
                let result_json = serde_json::to_value(&output).map_err(|e| {
                    StepError::new(
                        e.to_string(),
                        StepErrorKind::Unknown,
                        Self::TYPE_ID.to_string(),
                        false,
                    )
                })?;
                Ok(StepResult::Output(result_json))
            }
            Err(e) => Err(StepError::new(
                e.to_string(),
                StepErrorKind::ActivityFailed,
                Self::TYPE_ID.to_string(),
                true,
            )),
        }
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        // No compensation needed for validation step
        Ok(())
    }
}

/// Step for validating worker spec
pub struct ValidateWorkerSpecStep {
    activity: ValidateWorkerSpecActivity,
}

impl std::fmt::Debug for ValidateWorkerSpecStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateWorkerSpecStep")
            .field("activity", &self.activity)
            .finish()
    }
}

impl ValidateWorkerSpecStep {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            activity: ValidateWorkerSpecActivity::new(provisioning_service),
        }
    }
}

#[async_trait]
impl WorkflowStep for ValidateWorkerSpecStep {
    const TYPE_ID: &'static str = "provisioning-validate-spec";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: ProvisioningWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        // Get server address from context or use default
        let server_address = context
            .step_outputs
            .get("config")
            .and_then(|v| v.get("server_address"))
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:50051")
            .to_string();

        let activity_input = ValidateWorkerSpecInput {
            spec: input.spec,
            server_address,
        };

        match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input).await {
            Ok(output) => {
                let result_json = serde_json::to_value(&output).map_err(|e| {
                    StepError::new(
                        e.to_string(),
                        StepErrorKind::Unknown,
                        Self::TYPE_ID.to_string(),
                        false,
                    )
                })?;
                Ok(StepResult::Output(result_json))
            }
            Err(e) => Err(StepError::new(
                e.to_string(),
                StepErrorKind::ActivityFailed,
                Self::TYPE_ID.to_string(),
                true,
            )),
        }
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        // No compensation needed for spec validation
        Ok(())
    }
}

/// Step for provisioning worker
pub struct ProvisionWorkerStep {
    activity: ProvisionWorkerActivity,
}

impl std::fmt::Debug for ProvisionWorkerStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionWorkerStep")
            .field("activity", &self.activity)
            .finish()
    }
}

impl ProvisionWorkerStep {
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        Self {
            activity: ProvisionWorkerActivity::new(provisioning_service),
        }
    }
}

#[async_trait]
impl WorkflowStep for ProvisionWorkerStep {
    const TYPE_ID: &'static str = "provisioning-provision-worker";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: ProvisioningWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        // Get server address from context
        let server_address = context
            .step_outputs
            .get("config")
            .and_then(|v| v.get("server_address"))
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:50051")
            .to_string();

        let activity_input = ProvisionWorkerInput {
            provider_id: input.provider_id.clone(),
            spec: input.spec,
            server_address,
            job_id: input.job_id,
        };

        match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input).await {
            Ok(output) => {
                let result_json = serde_json::to_value(&output).map_err(|e| {
                    StepError::new(
                        e.to_string(),
                        StepErrorKind::Unknown,
                        Self::TYPE_ID.to_string(),
                        false,
                    )
                })?;
                Ok(StepResult::Output(result_json))
            }
            Err(e) => Err(StepError::new(
                e.to_string(),
                StepErrorKind::ActivityFailed,
                Self::TYPE_ID.to_string(),
                true,
            )),
        }
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        // Worker termination is handled by the worker lifecycle manager
        // This step doesn't need additional compensation
        Ok(())
    }
}

// =============================================================================
// Workflow Definition
// =============================================================================

/// Provisioning Workflow Definition for saga-engine v4
pub struct ProvisioningWorkflow {
    steps: Vec<Box<dyn DynWorkflowStep>>,
    config: WorkflowConfig,
}

impl std::fmt::Debug for ProvisioningWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisioningWorkflow")
            .field("steps", &format!("[{} steps]", self.steps.len()))
            .field("config", &self.config)
            .finish()
    }
}

impl ProvisioningWorkflow {
    /// Create a new ProvisioningWorkflow
    pub fn new(provisioning_service: Arc<dyn WorkerProvisioningService>) -> Self {
        let steps = vec![
            Box::new(ValidateProviderStep::new(provisioning_service.clone()))
                as Box<dyn DynWorkflowStep>,
            Box::new(ValidateWorkerSpecStep::new(provisioning_service.clone()))
                as Box<dyn DynWorkflowStep>,
            Box::new(ProvisionWorkerStep::new(provisioning_service.clone()))
                as Box<dyn DynWorkflowStep>,
        ];

        let config = WorkflowConfig {
            default_timeout_secs: 300,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "provisioning".to_string(),
        };

        Self { steps, config }
    }
}

impl WorkflowDefinition for ProvisioningWorkflow {
    const TYPE_ID: &'static str = "provisioning";
    const VERSION: u32 = 1;

    type Input = ProvisioningWorkflowInput;
    type Output = ProvisioningWorkflowOutput;

    fn steps(&self) -> &[Box<dyn DynWorkflowStep>] {
        &self.steps
    }

    fn configuration(&self) -> WorkflowConfig {
        self.config.clone()
    }
}

// =============================================================================
// Workflow Executor Builder
// =============================================================================

/// Builder for ProvisioningWorkflow executor
pub struct ProvisioningWorkflowExecutorBuilder {
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
}

impl std::fmt::Debug for ProvisioningWorkflowExecutorBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisioningWorkflowExecutorBuilder")
            .field("provisioning_service", &self.provisioning_service.is_some())
            .finish()
    }
}

impl ProvisioningWorkflowExecutorBuilder {
    pub fn new() -> Self {
        Self {
            provisioning_service: None,
        }
    }

    pub fn with_provisioning_service(
        mut self,
        service: Arc<dyn WorkerProvisioningService>,
    ) -> Self {
        self.provisioning_service = Some(service);
        self
    }

    pub fn build(self) -> Result<WorkflowExecutor<ProvisioningWorkflow>, BuildError> {
        let provisioning_service = self
            .provisioning_service
            .ok_or(BuildError::MissingService)?;

        let workflow = ProvisioningWorkflow::new(provisioning_service);
        Ok(WorkflowExecutor::new(workflow))
    }
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("WorkerProvisioningService is required")]
    MissingService,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provisioning_workflow_input() {
        let input = ProvisioningWorkflowInput::new(
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

        assert!(input.idempotency_key.is_some());
        assert_eq!(input.provider_id, "provider-1");
        assert_eq!(input.job_id, Some("job-123".to_string()));
    }

    #[test]
    fn test_provisioning_workflow_config() {
        let config = WorkflowConfig::default();
        assert_eq!(config.default_timeout_secs, 3600);
        assert_eq!(config.max_step_retries, 3);
    }

    #[test]
    fn test_step_result_serialization() {
        let result = StepResult::Output(serde_json::json!({"key": "value"}));
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: StepResult = serde_json::from_str(&json).unwrap();
        match deserialized {
            StepResult::Output(v) => assert_eq!(v, serde_json::json!({"key": "value"})),
            _ => panic!("Expected Output variant"),
        }
    }

    #[test]
    fn test_worker_spec_data_to_spec() {
        let input = ValidateWorkerSpecInput {
            spec: WorkerSpecData {
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
            server_address: "http://localhost:50051".to_string(),
        };

        let spec = input.to_worker_spec();
        assert_eq!(spec.image, "worker:latest");
        assert_eq!(spec.resources.cpu_cores, 2.0);
        assert_eq!(spec.server_address, "http://localhost:50051");
    }

    #[test]
    fn test_provision_worker_input_to_spec() {
        let input = ProvisionWorkerInput {
            provider_id: "provider-1".to_string(),
            spec: WorkerSpecData {
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
            server_address: "http://localhost:50051".to_string(),
            job_id: Some("job-123".to_string()),
        };

        let spec = input.to_worker_spec();
        assert_eq!(spec.image, "worker:latest");
        assert!(spec.environment.contains_key("KEY"));
        assert!(spec.labels.contains_key("label1"));
    }
}
