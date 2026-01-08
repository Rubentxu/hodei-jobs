//! gRPC Service Implementations
//!
//! This module contains the implementations of gRPC services that act as
//! adapters between the gRPC transport layer and the application layer.

pub mod audit;
pub mod correlation;
pub mod heartbeat_processor;
pub mod interceptors;
pub mod job_execution;
pub mod log_ingestor;
pub mod log_stream;
pub mod metrics;
pub mod provider_management;
pub mod scheduler;
pub mod worker;
pub mod worker_command_sender;

pub use audit::AuditServiceImpl;
pub use correlation::{CORRELATION_ID_HEADER, CorrelationIdManager, RequestCorrelationExt};
pub use heartbeat_processor::{HeartbeatProcessor, HeartbeatProcessorConfig, HeartbeatResult, WorkerSupervisorHandle};
pub use interceptors::context::context_interceptor;
pub use job_execution::JobExecutionServiceImpl;
pub use log_ingestor::{LogBatch, LogFinalizationResult, LogIngestionResult, LogIngestor, LogIngestorConfig, LogIngestorMetrics};
pub use log_stream::{LogStreamService, LogStreamServiceGrpc};
pub use metrics::MetricsServiceImpl;
pub use provider_management::ProviderManagementServiceImpl;
pub use scheduler::SchedulerServiceImpl;
pub use worker::WorkerAgentServiceImpl;
pub use worker_command_sender::GrpcWorkerCommandSender;
