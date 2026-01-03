//! Template Management Module for Event-Driven Architecture
//!
//! This module provides the complete implementation for Job Templates management:
//! - Commands: CreateTemplate, UpdateTemplate, DeleteTemplate, TriggerRun
//! - Queries: GetTemplate, ListTemplates, GetExecution, ListExecutions
//! - Handlers: Command handlers implementing business logic
//! - Read Models: Optimized views for reading template data
//!
//! # Architecture
//!
//! ```text
//! gRPC Service → CommandBus → CommandHandler → EventPublisher → EventHandlers
//!                                           ↓
//!                                     Read Models (updated async)
//! ```

pub mod commands;
pub mod handlers;
pub mod queries;
pub mod read_models;

// Re-exports
pub use commands::*;
pub use handlers::*;
pub use queries::*;
pub use read_models::*;

/// Repository ports for template storage
pub use hodei_server_domain::jobs::templates::{
    JobExecutionRepository, JobTemplateParameterRepository, JobTemplateRepository,
    ScheduledJobRepository,
};
