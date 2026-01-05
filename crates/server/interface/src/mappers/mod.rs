//! Domain-Protocol Buffer Mappers
//!
//! Centralized conversion functions between domain types and Protocol Buffer messages.
//! This module ensures consistent mapping across all gRPC services.

use hodei_jobs::job::ResourceRequirements;
use hodei_jobs::job::{JobSpec as GrpcJobSpec, JobTemplate, TemplateExecution};
use hodei_jobs::{
    ExecutionId, JobDefinition, JobExecution, JobStatus, JobSummary, LabelSelector, SchedulingInfo,
    TimeoutConfig, Toleration,
};
// Temporarily commented out until template module is fixed
// use hodei_server_application::jobs::template::queries::{ExecutionSummary, TemplateSummary};
use hodei_server_domain::jobs::{Job, JobSpec};
use hodei_server_domain::shared_kernel::JobState;
use prost_types::{Duration, Timestamp};
use std::time::Duration as StdDuration;

/// Convert chrono::DateTime<Utc> to prost_types::Timestamp
#[inline]
pub fn to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Convert Option<chrono::DateTime<Utc>> to Option<prost_types::Timestamp>
#[inline]
pub fn to_optional_timestamp(dt: Option<chrono::DateTime<chrono::Utc>>) -> Option<Timestamp> {
    dt.map(to_timestamp)
}

/// Convert chrono::Duration to prost_types::Duration
#[inline]
pub fn to_prost_duration(duration: chrono::Duration) -> Duration {
    Duration {
        seconds: duration.num_seconds(),
        nanos: (duration.num_milliseconds() % 1000 * 1_000_000) as i32,
    }
}

/// Convert std::time::Duration to prost_types::Duration
#[inline]
pub fn to_std_prost_duration(duration: StdDuration) -> Duration {
    Duration {
        seconds: duration.as_secs() as i64,
        nanos: duration.subsec_nanos() as i32,
    }
}

/// Convert Option<std::time::Duration> to Option<prost_types::Duration>
#[inline]
pub fn to_optional_std_prost_duration(duration: Option<StdDuration>) -> Option<Duration> {
    duration.map(to_std_prost_duration)
}

/// Get current timestamp as prost_types::Timestamp
#[inline]
pub fn now_timestamp() -> Timestamp {
    let now = chrono::Utc::now();
    Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    }
}

/// Map domain JobState to proto JobStatus
#[inline]
pub fn map_job_state(state: &JobState) -> JobStatus {
    match state {
        JobState::Pending => JobStatus::Pending,
        JobState::Assigned => JobStatus::Assigned,
        JobState::Scheduled => JobStatus::Queued,
        JobState::Running => JobStatus::Running,
        JobState::Succeeded => JobStatus::Completed,
        JobState::Failed => JobStatus::Failed,
        JobState::Cancelled => JobStatus::Cancelled,
        JobState::Timeout => JobStatus::Timeout,
    }
}

/// Convert domain JobPriority to proto PriorityLevel
pub fn map_job_priority(priority: &hodei_server_domain::jobs::JobPriority) -> i32 {
    match priority {
        hodei_server_domain::jobs::JobPriority::Low => 0, // PRIORITY_LOW
        hodei_server_domain::jobs::JobPriority::Normal => 1, // PRIORITY_NORMAL
        hodei_server_domain::jobs::JobPriority::High => 2, // PRIORITY_HIGH
        hodei_server_domain::jobs::JobPriority::Critical => 3, // PRIORITY_CRITICAL
    }
}

/// Map domain Job to proto JobSummary
pub fn map_job_to_summary(job: &Job) -> JobSummary {
    let duration = job.execution_duration().map(to_std_prost_duration);

    let progress = job
        .metadata()
        .get("progress_percentage")
        .and_then(|p| p.parse::<i32>().ok())
        .unwrap_or(0);

    let state = job.state();

    JobSummary {
        job_id: Some(hodei_jobs::JobId {
            value: job.id.to_string(),
        }),
        name: format!("Job {}", &job.id.to_string()[..8]),
        status: map_job_state(state) as i32,
        created_at: Some(to_timestamp(*job.created_at())),
        started_at: job.started_at().copied().map(to_timestamp),
        completed_at: job.completed_at().copied().map(to_timestamp),
        duration,
        progress_percentage: progress,
    }
}

/// Map domain JobSpec to proto JobDefinition
pub fn map_spec_to_definition(spec: &JobSpec, job_id: &str) -> JobDefinition {
    let cmd_vec = spec.command_vec();
    let command = cmd_vec.first().cloned().unwrap_or_default();
    let arguments = if cmd_vec.len() > 1 {
        cmd_vec[1..].to_vec()
    } else {
        vec![]
    };

    let scheduling = SchedulingInfo {
        priority: map_job_priority(&spec.preferences.priority),
        scheduler_name: String::new(),
        deadline: None,
        preemption_allowed: false,
        preferred_provider: spec
            .preferences
            .preferred_provider
            .clone()
            .unwrap_or_default(),
        required_labels: spec.preferences.required_labels.clone(),
        required_annotations: spec.preferences.required_annotations.clone(),
        preferred_region: spec
            .preferences
            .preferred_region
            .clone()
            .unwrap_or_default(),
    };

    let timeout = TimeoutConfig {
        execution_timeout: Some(Duration {
            seconds: spec.timeout_ms as i64 / 1000,
            nanos: ((spec.timeout_ms as i64 % 1000) * 1_000_000) as i32,
        }),
        heartbeat_timeout: None,
        cleanup_timeout: None,
    };

    JobDefinition {
        job_id: Some(hodei_jobs::JobId {
            value: job_id.to_string(),
        }),
        name: format!("Job {}", &job_id[..8]),
        description: String::new(),
        command,
        arguments,
        environment: spec.env.clone(),
        requirements: Some(hodei_jobs::ResourceRequirements {
            cpu_cores: spec.resources.cpu_cores as f64,
            memory_bytes: (spec.resources.memory_mb * 1024 * 1024) as i64,
            disk_bytes: (spec.resources.storage_mb * 1024 * 1024) as i64,
            gpu_count: if spec.resources.gpu_required { 1 } else { 0 },
            gpu_types: vec![],
            custom_required: std::collections::HashMap::new(),
        }),
        scheduling: Some(scheduling),
        selector: None,
        tolerations: vec![],
        timeout: Some(timeout),
        tags: vec![],
    }
}

/// Map domain Job to proto JobDefinition
pub fn map_job_to_definition(job: &Job) -> JobDefinition {
    let spec = &job.spec;
    map_spec_to_definition(spec, &job.id.to_string())
}

/// Map domain Job to proto JobExecution
pub fn map_job_to_execution(job: &Job) -> JobExecution {
    let state = job.state();

    // Map result to exit_code and error_message
    let (exit_code, error_message) = match job.result() {
        Some(hodei_server_domain::shared_kernel::JobResult::Success { exit_code, .. }) => {
            (exit_code.to_string(), String::new())
        }
        Some(hodei_server_domain::shared_kernel::JobResult::Failed {
            exit_code,
            error_message,
            ..
        }) => (exit_code.to_string(), error_message.clone()),
        Some(hodei_server_domain::shared_kernel::JobResult::Cancelled) => {
            ((-1i32).to_string(), "Cancelled".to_string())
        }
        Some(hodei_server_domain::shared_kernel::JobResult::Timeout) => {
            ((-1i32).to_string(), "Timeout".to_string())
        }
        None => (String::new(), String::new()),
    };

    JobExecution {
        execution_id: Some(ExecutionId {
            value: job
                .execution_context()
                .map(|ctx| ctx.provider_execution_id.clone())
                .unwrap_or_default(),
        }),
        job_id: Some(hodei_jobs::JobId {
            value: job.id.to_string(),
        }),
        worker_id: None, // Would need worker info
        state: 0,        // ExecutionState::CREATED
        job_status: map_job_state(state) as i32,
        start_time: job.started_at().copied().map(to_timestamp),
        end_time: job.completed_at().copied().map(to_timestamp),
        retry_count: job.attempts() as i32,
        exit_code,
        error_message,
        metadata: job.metadata().clone(),
    }
}

/// Parse gRPC JobId to domain JobId
#[inline]
pub fn parse_grpc_job_id(
    job_id: Option<hodei_jobs::JobId>,
) -> Result<hodei_server_domain::shared_kernel::JobId, tonic::Status> {
    let value = job_id
        .map(|id| id.value)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| tonic::Status::invalid_argument("job_id is required"))?;

    let uuid = uuid::Uuid::parse_str(&value)
        .map_err(|_| tonic::Status::invalid_argument("job_id must be a UUID"))?;
    Ok(hodei_server_domain::shared_kernel::JobId(uuid))
}

/// Convert anyhow::Error to tonic::Status
pub fn error_to_status(err: impl std::fmt::Display) -> tonic::Status {
    let err_str = err.to_string();
    if err_str.contains("not found") || err_str.contains("NotFound") {
        tonic::Status::not_found(err_str)
    } else if err_str.contains("not available") || err_str.contains("NotAvailable") {
        tonic::Status::failed_precondition(err_str)
    } else if err_str.contains("invalid") || err_str.contains("Invalid") {
        tonic::Status::invalid_argument(err_str)
    } else {
        tonic::Status::internal(err_str)
    }
}

/// Standardized conversion from DomainError to tonic::Status
///
/// This implementation provides type-safe error mapping instead of string-based detection.
impl From<hodei_server_domain::shared_kernel::DomainError> for tonic::Status {
    fn from(err: hodei_server_domain::shared_kernel::DomainError) -> Self {
        use hodei_server_domain::shared_kernel::DomainError;

        match err {
            // Not Found errors
            DomainError::JobNotFound { job_id } => {
                tonic::Status::not_found(format!("Job '{}' not found", job_id))
            }
            DomainError::WorkerNotFound { worker_id } => {
                tonic::Status::not_found(format!("Worker '{}' not found", worker_id))
            }
            DomainError::ProviderNotFound { provider_id } => {
                tonic::Status::not_found(format!("Provider '{}' not found", provider_id))
            }

            // Resource exhausted
            DomainError::ProviderOverloaded { provider_id, .. } => {
                tonic::Status::resource_exhausted(format!(
                    "Provider '{}' is overloaded, retry later",
                    provider_id
                ))
            }

            // Invalid argument
            DomainError::InvalidJobState {
                current,
                expected,
                job_id,
            } => tonic::Status::failed_precondition(format!(
                "Invalid job state for '{}': expected {:?}, got {:?}",
                job_id, expected, current
            )),
            DomainError::ValidationError { message } => tonic::Status::invalid_argument(message),

            // Already exists
            DomainError::JobAlreadyExists { job_id } => {
                tonic::Status::already_exists(format!("Job '{}' already exists", job_id))
            }

            // Permission denied
            DomainError::Unauthorized { reason } => tonic::Status::permission_denied(reason),

            // Timeout
            DomainError::Timeout { operation, .. } => {
                tonic::Status::deadline_exceeded(format!("Operation '{}' timed out", operation))
            }

            // Conflict
            DomainError::Conflict { message } => tonic::Status::conflict(message),

            // Catch-all for internal errors
            _ => tonic::Status::internal(err.to_string()),
        }
    }
}

/*
/// Map TemplateSummary to gRPC JobTemplate
pub fn map_template_summary_to_grpc(
    summary: &TemplateSummary,
) -> Result<JobTemplate, tonic::Status> {
    Ok(JobTemplate {
        template_id: summary.id.to_string(),
        name: summary.name.clone(),
        description: summary.description.clone().unwrap_or_default(),
        spec: GrpcJobSpec::default(), // TODO: Get spec from template
        status: summary.status.clone(),
        version: summary.version,
        labels: summary.labels.clone(),
        created_at: Some(to_timestamp(summary.created_at)),
        updated_at: Some(to_timestamp(summary.updated_at)),
        created_by: summary.created_by.clone().unwrap_or_default(),
        run_count: summary.run_count,
        success_count: summary.success_count,
        failure_count: summary.failure_count,
    })
}

/// Map ExecutionSummary to gRPC TemplateExecution
pub fn map_execution_summary_to_grpc(
    summary: &ExecutionSummary,
) -> Result<TemplateExecution, tonic::Status> {
    Ok(TemplateExecution {
        execution_id: summary.id.to_string(),
        execution_number: summary.execution_number,
        template_id: summary.template_id.to_string(),
        template_version: summary.template_version,
        job_id: summary.job_id.as_ref().map(|j| j.to_string()),
        job_name: summary.job_name.clone(),
        job_spec: GrpcJobSpec::default(), // TODO: Get spec from execution
        state: summary.state.clone(),
        result: None, // TODO: Map execution result
        queued_at: Some(to_timestamp(summary.queued_at)),
        started_at: summary.started_at.map(to_timestamp),
        completed_at: summary.completed_at.map(to_timestamp),
        triggered_by: summary.triggered_by.clone(),
        scheduled_job_id: None,
        triggered_by_user: summary.triggered_by_user.clone(),
        parameters: summary.parameters.clone(),
        resource_usage: None,
        metadata: std::collections::HashMap::new(),
        created_at: Some(to_timestamp(summary.queued_at)),
    })
}
*/

/// Map gRPC JobSpec to domain JobSpec (JSON value)
pub fn map_job_spec_to_grpc(spec: &JobSpec) -> GrpcJobSpec {
    GrpcJobSpec {
        command: spec.command_vec().first().cloned().unwrap_or_default(),
        arguments: if spec.command_vec().len() > 1 {
            spec.command_vec()[1..].to_vec()
        } else {
            vec![]
        },
        environment: spec.env.clone(),
        inputs: vec![],      // TODO: Map inputs
        outputs: vec![],     // TODO: Map outputs
        constraints: vec![], // TODO: Map constraints
        resources: Some(hodei_jobs::job::ResourceRequirements {
            cpu_cores: spec.resources.cpu_cores,
            memory_mb: spec.resources.memory_mb,
            storage_mb: spec.resources.storage_mb,
            gpu_required: spec.resources.gpu_required,
            architecture: spec.resources.architecture.clone(),
        }),
        timeout_ms: spec.timeout_ms,
        image: spec.image.clone().unwrap_or_default(),
        working_dir: spec.working_dir.clone().unwrap_or_default(),
        stdin: spec.stdin.clone().unwrap_or_default(),
        preferences: None, // TODO: Map preferences
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::{Job, JobSpec};
    use hodei_server_domain::shared_kernel::JobId;

    #[test]
    fn test_map_job_state() {
        assert_eq!(map_job_state(&JobState::Pending), JobStatus::Pending);
        assert_eq!(map_job_state(&JobState::Assigned), JobStatus::Assigned);
        assert_eq!(map_job_state(&JobState::Scheduled), JobStatus::Queued);
        assert_eq!(map_job_state(&JobState::Running), JobStatus::Running);
        assert_eq!(map_job_state(&JobState::Succeeded), JobStatus::Completed);
        assert_eq!(map_job_state(&JobState::Failed), JobStatus::Failed);
        assert_eq!(map_job_state(&JobState::Cancelled), JobStatus::Cancelled);
        assert_eq!(map_job_state(&JobState::Timeout), JobStatus::Timeout);
    }

    #[test]
    fn test_map_job_to_summary() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()]);
        let job = Job::new(job_id.clone(), spec);

        let summary = map_job_to_summary(&job);

        assert!(summary.job_id.is_some());
        assert_eq!(summary.status, JobStatus::Pending as i32);
        assert!(summary.created_at.is_some());
        assert!(summary.started_at.is_none());
        assert!(summary.completed_at.is_none());
    }

    #[test]
    fn test_to_timestamp() {
        let dt = chrono::DateTime::from_timestamp(1234567890, 0).unwrap();
        let ts = to_timestamp(dt);

        assert_eq!(ts.seconds, 1234567890);
        assert_eq!(ts.nanos, 0);
    }

    #[test]
    fn test_now_timestamp() {
        let ts = now_timestamp();
        let now = chrono::Utc::now();

        // Should be very close to current time
        let diff = (now.timestamp() - ts.seconds).abs();
        assert!(diff <= 1);
    }

    #[test]
    fn test_to_std_prost_duration() {
        let duration = StdDuration::from_secs(120) + StdDuration::from_nanos(500_000_000);
        let prost = to_std_prost_duration(duration);

        assert_eq!(prost.seconds, 120);
        assert_eq!(prost.nanos, 500_000_000);
    }

    #[test]
    fn test_parse_grpc_job_id_valid() {
        let job_id = hodei_jobs::JobId {
            value: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        };

        let result = parse_grpc_job_id(Some(job_id));
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_grpc_job_id_invalid() {
        let result = parse_grpc_job_id(None);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_grpc_job_id_empty() {
        let job_id = hodei_jobs::JobId {
            value: String::new(),
        };
        let result = parse_grpc_job_id(Some(job_id));
        assert!(result.is_err());
    }
}
