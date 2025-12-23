//! Unit tests for domain layer
use crate::jobs::*;
use crate::shared_kernel::*;

mod shared_kernel_tests {
    use super::*;

    #[test]
    fn test_job_id_creation() {
        let id = JobId::new();
        assert!(!id.0.is_nil());
    }

    #[test]
    fn test_job_id_default() {
        let id = JobId::default();
        assert!(!id.0.is_nil());
    }

    #[test]
    fn test_job_id_display() {
        let id = JobId::new();
        let display = format!("{}", id);
        assert!(!display.is_empty());
        assert_eq!(display.len(), 36);
    }

    #[test]
    fn test_job_id_equality() {
        let uuid = uuid::Uuid::new_v4();
        let id1 = JobId(uuid);
        let id2 = JobId(uuid);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_job_id_inequality() {
        let id1 = JobId::new();
        let id2 = JobId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_provider_id_creation() {
        let id = ProviderId::new();
        assert!(!id.0.is_nil());
    }

    #[test]
    fn test_correlation_id_creation() {
        let id = CorrelationId::new();
        assert!(!id.0.is_nil());
    }

    #[test]
    fn test_job_state_display() {
        assert_eq!(format!("{}", JobState::Pending), "PENDING");
        assert_eq!(format!("{}", JobState::Running), "RUNNING");
        assert_eq!(format!("{}", JobState::Succeeded), "SUCCEEDED");
        assert_eq!(format!("{}", JobState::Failed), "FAILED");
        assert_eq!(format!("{}", JobState::Cancelled), "CANCELLED");
    }

    #[test]
    fn test_provider_status_display() {
        assert_eq!(format!("{}", ProviderStatus::Active), "ACTIVE");
        assert_eq!(format!("{}", ProviderStatus::Maintenance), "MAINTENANCE");
        assert_eq!(format!("{}", ProviderStatus::Unhealthy), "UNHEALTHY");
    }

    #[test]
    fn test_execution_status_display() {
        assert_eq!(format!("{}", ExecutionStatus::Running), "RUNNING");
        assert_eq!(format!("{}", ExecutionStatus::Succeeded), "SUCCEEDED");
        assert_eq!(format!("{}", ExecutionStatus::Failed), "FAILED");
    }

    #[test]
    fn test_job_result_success_display() {
        let result = JobResult::Success {
            exit_code: 0,
            output: "Hello".to_string(),
            error_output: "".to_string(),
        };
        let display = format!("{}", result);
        assert!(display.contains("SUCCESS"));
    }

    #[test]
    fn test_job_result_failed_display() {
        let result = JobResult::Failed {
            exit_code: 1,
            error_message: "Error".to_string(),
            error_output: "".to_string(),
        };
        let display = format!("{}", result);
        assert!(display.contains("FAILED"));
    }

    #[test]
    fn test_domain_error_job_not_found() {
        let job_id = JobId::new();
        let error = DomainError::JobNotFound { job_id };
        let msg = format!("{}", error);
        assert!(msg.contains("Job not found"));
    }

    #[test]
    fn test_domain_error_invalid_state_transition() {
        let error = DomainError::InvalidStateTransition {
            job_id: JobId::new(),
            from_state: JobState::Pending,
            to_state: JobState::Succeeded,
        };
        let msg = format!("{}", error);
        assert!(msg.contains("Invalid job state transition"));
    }
}

mod job_execution_tests {
    use super::*;

    fn create_test_job() -> Job {
        let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()]);
        Job::new(JobId::new(), spec)
    }

    #[test]
    fn test_job_spec_new() {
        let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);
        // EPIC-21 Jenkins sh behavior: commands are wrapped as bash -c "command"
        let cmd_vec = spec.command_vec();
        assert_eq!(cmd_vec[0], "bash");
        assert_eq!(cmd_vec[1], "-c");
        assert!(cmd_vec[2].contains("echo"));
        assert!(cmd_vec[2].contains("hello"));
        assert_eq!(spec.timeout_ms, 300_000);
    }

    #[test]
    fn test_job_resources_default() {
        let resources = JobResources::default();
        assert_eq!(resources.cpu_cores, 1.0);
        assert_eq!(resources.memory_mb, 512);
        assert!(!resources.gpu_required);
    }

    #[test]
    fn test_job_priority_display() {
        assert_eq!(format!("{}", JobPriority::Low), "LOW");
        assert_eq!(format!("{}", JobPriority::Normal), "NORMAL");
        assert_eq!(format!("{}", JobPriority::High), "HIGH");
        assert_eq!(format!("{}", JobPriority::Critical), "CRITICAL");
    }

    #[test]
    fn test_job_creation() {
        let job = create_test_job();
        assert_eq!(*job.state(), JobState::Pending);
        assert_eq!(job.attempts(), 0);
        assert_eq!(job.max_attempts(), 3);
    }

    #[test]
    fn test_job_queue() {
        let mut job = create_test_job();
        assert!(job.queue().is_ok());
        assert_eq!(*job.state(), JobState::Pending);
    }

    #[test]
    fn test_job_submit_to_provider() {
        let mut job = create_test_job();
        let provider_id = ProviderId::new();
        let context =
            ExecutionContext::new(job.id.clone(), provider_id.clone(), "exec-123".to_string());

        assert!(job.submit_to_provider(provider_id.clone(), context).is_ok());
        // PRD v6.0: Submitted -> Scheduled
        assert_eq!(*job.state(), JobState::Scheduled);
        assert!(job.started_at().is_some());
    }

    #[test]
    fn test_job_mark_running() {
        let mut job = create_test_job();
        let provider_id = ProviderId::new();
        let context =
            ExecutionContext::new(job.id.clone(), provider_id.clone(), "exec-123".to_string());

        job.submit_to_provider(provider_id, context).unwrap();
        assert!(job.mark_running().is_ok());
        assert_eq!(*job.state(), JobState::Running);
    }

    #[test]
    fn test_job_mark_running_invalid_state() {
        let mut job = create_test_job();
        let result = job.mark_running();
        assert!(result.is_err());
    }

    #[test]
    fn test_job_complete_success() {
        let mut job = create_test_job();
        let provider_id = ProviderId::new();
        let context =
            ExecutionContext::new(job.id.clone(), provider_id.clone(), "exec-123".to_string());

        job.submit_to_provider(provider_id, context).unwrap();
        job.mark_running().unwrap();

        let result = JobResult::Success {
            exit_code: 0,
            output: "Success".to_string(),
            error_output: "".to_string(),
        };

        assert!(job.complete(result).is_ok());
        assert_eq!(*job.state(), JobState::Succeeded);
    }

    #[test]
    fn test_job_fail() {
        let mut job = create_test_job();
        assert!(job.fail("Test error".to_string()).is_ok());
        assert_eq!(*job.state(), JobState::Failed);
        assert_eq!(job.attempts(), 1);
    }

    #[test]
    fn test_job_cancel() {
        let mut job = create_test_job();
        assert!(job.cancel().is_ok());
        assert_eq!(*job.state(), JobState::Cancelled);
    }

    #[test]
    fn test_job_can_retry() {
        let mut job = create_test_job();
        job.fail("Error".to_string()).unwrap();
        assert!(job.can_retry());
    }

    #[test]
    fn test_job_cannot_retry_max_attempts() {
        let mut job = create_test_job();
        job.set_attempts(3);
        job.set_state(JobState::Failed);
        assert!(!job.can_retry());
    }

    #[test]
    fn test_job_prepare_retry() {
        let mut job = create_test_job();
        job.fail("Error".to_string()).unwrap();

        assert!(job.prepare_retry().is_ok());
        assert_eq!(*job.state(), JobState::Pending);
        assert_eq!(job.attempts(), 2);
    }

    #[test]
    fn test_execution_context_creation() {
        let job_id = JobId::new();
        let provider_id = ProviderId::new();
        let context =
            ExecutionContext::new(job_id.clone(), provider_id.clone(), "exec-123".to_string());

        assert_eq!(context.job_id, job_id);
        assert_eq!(context.provider_id, provider_id);
        assert_eq!(context.status, ExecutionStatus::Submitted);
    }

    #[test]
    fn test_execution_context_update_status() {
        let mut context =
            ExecutionContext::new(JobId::new(), ProviderId::new(), "exec-123".to_string());

        context.update_status(ExecutionStatus::Running);
        assert_eq!(context.status, ExecutionStatus::Running);
        assert!(context.started_at.is_some());
    }
}

mod provider_type_tests {
    use crate::workers::ProviderCapabilities;
    use crate::workers::ProviderType;

    #[test]
    fn test_provider_type_display() {
        assert_eq!(format!("{}", ProviderType::Lambda), "lambda");
        assert_eq!(format!("{}", ProviderType::Kubernetes), "kubernetes");
        assert_eq!(format!("{}", ProviderType::Docker), "docker");
    }

    #[test]
    fn test_provider_type_custom() {
        let custom = ProviderType::Custom("myworker".to_string());
        assert_eq!(format!("{}", custom), "custom:myworker");
    }

    #[test]
    fn test_provider_capabilities_default() {
        let caps = ProviderCapabilities::default();
        assert_eq!(caps.max_resources.max_cpu_cores, 4.0);
        assert_eq!(caps.max_resources.max_memory_bytes, 8 * 1024 * 1024 * 1024);
        assert!(!caps.gpu_support);
    }
}

mod worker_tests {
    use super::*;
    use crate::workers::{ProviderCategory, ProviderType, Worker, WorkerHandle, WorkerSpec};

    #[test]
    fn test_provider_type_category() {
        assert_eq!(ProviderType::Docker.category(), ProviderCategory::Container);
        assert_eq!(
            ProviderType::Kubernetes.category(),
            ProviderCategory::Container
        );
        assert_eq!(
            ProviderType::Lambda.category(),
            ProviderCategory::Serverless
        );
        assert_eq!(
            ProviderType::EC2.category(),
            ProviderCategory::VirtualMachine
        );
    }

    #[test]
    fn test_worker_state_transitions() {
        let spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-123".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker = Worker::new(handle, spec);

        // PRD v6.0: Estado inicial es Creating
        assert_eq!(*worker.state(), WorkerState::Creating);

        worker.mark_connecting().unwrap();
        assert_eq!(*worker.state(), WorkerState::Connecting);

        worker.mark_ready().unwrap();
        assert!(worker.state().can_accept_jobs());
    }
}
