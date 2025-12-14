//! Unit tests for gRPC services
use crate::GrpcError;
use tonic::Status;

mod grpc_error_tests {
    use super::*;

    #[test]
    fn test_grpc_error_conversion_validation() {
        let error = GrpcError::ValidationError {
            message: "Invalid argument".to_string(),
        };

        let status: Status = error.into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_grpc_error_conversion_not_found() {
        let error = GrpcError::NotFound {
            message: "Resource not found".to_string(),
        };

        let status: Status = error.into();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[test]
    fn test_grpc_error_conversion_internal() {
        let error = GrpcError::InternalError {
            message: "Internal error".to_string(),
        };

        let status: Status = error.into();
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn test_grpc_error_conversion_resource() {
        let error = GrpcError::ResourceError {
            message: "Resource error".to_string(),
        };

        let status: Status = error.into();
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn test_grpc_error_conversion_service() {
        let error = GrpcError::ServiceError {
            message: "Service unavailable".to_string(),
        };

        let status: Status = error.into();
        assert_eq!(status.code(), tonic::Code::Unavailable);
    }

    #[test]
    fn test_grpc_error_from_anyhow() {
        let anyhow_error = anyhow::anyhow!("Something went wrong");
        let grpc_error: GrpcError = anyhow_error.into();
        
        match grpc_error {
            GrpcError::InternalError { message } => {
                assert!(message.contains("Something went wrong"));
            }
            _ => panic!("Expected InternalError"),
        }
    }

    #[test]
    fn test_grpc_error_display() {
        let error = GrpcError::ValidationError {
            message: "test error".to_string(),
        };
        let display = format!("{}", error);
        assert!(display.contains("Validation error"));
        assert!(display.contains("test error"));
    }
}

mod service_creation_tests {
    use crate::services::{
        WorkerAgentServiceImpl, JobExecutionServiceImpl, 
        MetricsServiceImpl, SchedulerServiceImpl,
    };
    use hodei_jobs_application::job_execution_usecases::{CancelJobUseCase, CreateJobUseCase};
    use hodei_jobs_application::smart_scheduler::SchedulerConfig;
    use hodei_jobs_infrastructure::repositories::{
        InMemoryJobQueue, InMemoryJobRepository, InMemoryWorkerRegistry,
    };

    #[test]
    fn test_worker_service_creation() {
        let _service = WorkerAgentServiceImpl::new();
        // Service should be created successfully
    }

    #[test]
    fn test_job_execution_service_creation() {
        let job_repository = std::sync::Arc::new(InMemoryJobRepository::new())
            as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
        let job_queue = std::sync::Arc::new(InMemoryJobQueue::new())
            as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;
        let worker_registry = std::sync::Arc::new(InMemoryWorkerRegistry::new())
            as std::sync::Arc<dyn hodei_jobs_domain::worker_registry::WorkerRegistry>;

        let create_job_usecase = CreateJobUseCase::new(job_repository.clone(), job_queue);
        let cancel_job_usecase = CancelJobUseCase::new(job_repository.clone());

        let _service = JobExecutionServiceImpl::new(
            std::sync::Arc::new(create_job_usecase),
            std::sync::Arc::new(cancel_job_usecase),
            job_repository,
            worker_registry,
        );
        // Service should be created successfully
    }

    #[test]
    fn test_metrics_service_creation() {
        let _service = MetricsServiceImpl::new();
        // Service should be created successfully
    }

    #[test]
    fn test_scheduler_service_creation() {
        let job_repository = std::sync::Arc::new(InMemoryJobRepository::new())
            as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
        let job_queue = std::sync::Arc::new(InMemoryJobQueue::new())
            as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;
        let worker_registry = std::sync::Arc::new(InMemoryWorkerRegistry::new())
            as std::sync::Arc<dyn hodei_jobs_domain::worker_registry::WorkerRegistry>;

        let create_job_usecase = CreateJobUseCase::new(job_repository.clone(), job_queue.clone());

        let _service = SchedulerServiceImpl::new(
            std::sync::Arc::new(create_job_usecase),
            job_repository,
            job_queue,
            worker_registry,
            SchedulerConfig::default(),
        );
        // Service should be created successfully
    }
}

/// Tests for OTP Token Service (PRD v6.0)
mod otp_token_tests {
    use crate::services::WorkerAgentServiceImpl;

    #[tokio::test]
    async fn test_generate_otp_returns_unique_tokens() {
        let service = WorkerAgentServiceImpl::new();
        let worker_id1 = uuid::Uuid::new_v4().to_string();
        let worker_id2 = uuid::Uuid::new_v4().to_string();
        
        let token1 = service.generate_otp(&worker_id1).await.expect("generate_otp should succeed");
        let token2 = service.generate_otp(&worker_id2).await.expect("generate_otp should succeed");
        
        assert!(!token1.is_empty());
        assert!(!token2.is_empty());
        assert_ne!(token1, token2);
    }

    #[tokio::test]
    async fn test_generate_otp_format_is_uuid() {
        let service = WorkerAgentServiceImpl::new();
        let worker_id = uuid::Uuid::new_v4().to_string();
        let token = service.generate_otp(&worker_id).await.expect("generate_otp should succeed");
        
        // UUID format: 8-4-4-4-12 characters
        assert_eq!(token.len(), 36);
        assert!(uuid::Uuid::parse_str(&token).is_ok());
    }

    #[tokio::test]
    async fn test_otp_can_be_used_for_registration() {
        use hodei_jobs::{RegisterWorkerRequest, WorkerInfo, WorkerId};
        use hodei_jobs::worker_agent_service_server::WorkerAgentService;
        use tonic::Request;
        
        let service = WorkerAgentServiceImpl::new();
        let worker_id = uuid::Uuid::new_v4().to_string();
        
        // Generate OTP
        let token = service.generate_otp(&worker_id).await.expect("generate_otp should succeed");
        
        // Use OTP for registration
        let request = Request::new(RegisterWorkerRequest {
            auth_token: token,
            session_id: String::new(),
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId { value: worker_id.clone() }),
                name: "Test Worker".to_string(),
                version: "1.0.0".to_string(),
                hostname: "localhost".to_string(),
                ip_address: "127.0.0.1".to_string(),
                os_info: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                capacity: None,
                capabilities: vec!["docker".to_string()],
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: None,
            }),
        });
        
        let response = service.register(request).await;
        assert!(response.is_ok());
        
        let resp = response.unwrap().into_inner();
        assert!(resp.success);
        assert!(!resp.session_id.is_empty());
    }

    #[tokio::test]
    async fn test_invalid_otp_rejected() {
        use hodei_jobs::{RegisterWorkerRequest, WorkerInfo, WorkerId};
        use hodei_jobs::worker_agent_service_server::WorkerAgentService;
        use tonic::Request;
        
        let service = WorkerAgentServiceImpl::new();
        let worker_id = uuid::Uuid::new_v4().to_string();
        
        // Try to register with invalid token
        let request = Request::new(RegisterWorkerRequest {
            auth_token: "invalid-token-that-does-not-exist".to_string(),
            session_id: String::new(),
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId { value: worker_id }),
                name: "Test Worker".to_string(),
                version: "1.0.0".to_string(),
                hostname: "localhost".to_string(),
                ip_address: "127.0.0.1".to_string(),
                os_info: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                capacity: None,
                capabilities: vec![],
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: None,
            }),
        });
        
        let response = service.register(request).await;
        assert!(response.is_err());
        
        let status = response.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn test_otp_cannot_be_reused() {
        use hodei_jobs::{RegisterWorkerRequest, WorkerInfo, WorkerId};
        use hodei_jobs::worker_agent_service_server::WorkerAgentService;
        use tonic::Request;
        
        let service = WorkerAgentServiceImpl::new();
        let worker_id = uuid::Uuid::new_v4().to_string();
        
        // Generate OTP
        let token = service.generate_otp(&worker_id).await.expect("generate_otp should succeed");
        
        // First registration should succeed
        let request1 = Request::new(RegisterWorkerRequest {
            auth_token: token.clone(),
            session_id: String::new(),
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId { value: worker_id.clone() }),
                name: "Test Worker".to_string(),
                version: "1.0.0".to_string(),
                hostname: "localhost".to_string(),
                ip_address: "127.0.0.1".to_string(),
                os_info: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                capacity: None,
                capabilities: vec![],
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: None,
            }),
        });
        
        let response1 = service.register(request1).await;
        assert!(response1.is_ok());
        
        // Second registration with same token should fail
        let different_worker_id = uuid::Uuid::new_v4().to_string();
        let request2 = Request::new(RegisterWorkerRequest {
            auth_token: token,
            session_id: String::new(),
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId { value: different_worker_id }),
                name: "Another Worker".to_string(),
                version: "1.0.0".to_string(),
                hostname: "localhost".to_string(),
                ip_address: "127.0.0.1".to_string(),
                os_info: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                capacity: None,
                capabilities: vec![],
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: None,
            }),
        });
        
        let response2 = service.register(request2).await;
        assert!(response2.is_err());
        
        let status = response2.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn test_session_id_generated_on_registration() {
        use hodei_jobs::{RegisterWorkerRequest, WorkerInfo, WorkerId};
        use hodei_jobs::worker_agent_service_server::WorkerAgentService;
        use tonic::Request;
        
        let service = WorkerAgentServiceImpl::new();
        let worker_id = uuid::Uuid::new_v4().to_string();
        let token = service.generate_otp(&worker_id).await.expect("generate_otp should succeed");
        
        let request = Request::new(RegisterWorkerRequest {
            auth_token: token,
            session_id: String::new(),
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId { value: worker_id }),
                name: "Test Worker".to_string(),
                version: "1.0.0".to_string(),
                hostname: "localhost".to_string(),
                ip_address: "127.0.0.1".to_string(),
                os_info: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                capacity: None,
                capabilities: vec![],
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: None,
            }),
        });
        
        let response = service.register(request).await.unwrap().into_inner();
        
        assert!(response.success);
        assert!(response.session_id.starts_with("sess_"));
        assert!(response.session_id.len() > 10);
    }
}
