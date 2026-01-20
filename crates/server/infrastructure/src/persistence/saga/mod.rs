//! Saga Infrastructure Module
//!
//! Provides saga-related infrastructure components:
//! - NotifyingSagaRepository: Wrapper that emits signals on saga changes
//! - ReactiveSagaProcessor: Processes sagas reactively without polling
//! - PostgresMigrationHelper: Persistent migration helper for production
//! - PostgresSagaTypeRegistry: Persistent registry for saga types

pub mod migration_helper;
pub mod notifying_repository;
pub mod reactive_processor;

pub use migration_helper::{
    PostgresMigrationHelper, PostgresMigrationHelperError, PostgresSagaTypeRegistry,
};
pub use notifying_repository::{
    NotifyingRepositoryMetrics, NotifyingSagaRepository, NotifyingSagaRepositoryBuilder,
    create_signal_channel,
};

pub use reactive_processor::{
    ReactiveSagaProcessor, ReactiveSagaProcessorConfig, ReactiveSagaProcessorMetrics,
};
