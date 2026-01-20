//! Saga Infrastructure Module
//!
//! Provides saga-related infrastructure components:
//! - NotifyingSagaRepository: Wrapper that emits signals on saga changes
//! - ReactiveSagaProcessor: Processes sagas reactively without polling

pub mod notifying_repository;
pub mod reactive_processor;

pub use notifying_repository::{
    NotifyingRepositoryMetrics, NotifyingSagaRepository, NotifyingSagaRepositoryBuilder,
    create_signal_channel,
};

pub use reactive_processor::{
    ReactiveSagaProcessor, ReactiveSagaProcessorConfig, ReactiveSagaProcessorMetrics,
};
