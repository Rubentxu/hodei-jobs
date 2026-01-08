//! Template Bounded Context
//!
//! EPIC-34: Job templates and scheduled jobs management.
//!
//! This module provides:
//! - Template creation, update, and lifecycle management
//! - Scheduled job execution with cron expressions
//! - Execution tracking and history
//!
//! ## Architecture
//!
//! The templates bounded context is organized as follows:
//!
//! - `events.rs`: Domain events for templates and scheduled jobs
//! - `mod.rs`: Module entry point and re-exports

pub mod events;

// Re-exports from events module
pub use events::{
    ExecutionRecorded, ScheduledJobCreated, ScheduledJobError, ScheduledJobMissed,
    ScheduledJobTriggered, TemplateCreated, TemplateDisabled, TemplateRunCreated, TemplateUpdated,
};
