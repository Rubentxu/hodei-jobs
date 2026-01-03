//! Core Infrastructure for Event-Driven Architecture
//!
//! Provides foundational infrastructure for CQRS and Event Sourcing patterns.

pub mod command;
pub mod event;
pub mod query;

pub use command::{
    Command, CommandBus, CommandBusConfig, CommandHandler, InMemoryCommandBus, ValidatableCommand,
};
pub use event::{
    DeadLetterQueue, EventBusConfig, EventFactory, EventHandler, EventMetadata, EventPublisher,
    InMemoryEventBus,
};
pub use query::{
    FilterCondition, FilterOperator, PaginatedResult, Pagination, Query, QueryHandler,
    QueryOptions, SortDirection, SortSpec, ValidatableQuery,
};

/// Core configuration
#[derive(Debug, Clone, Default)]
pub struct CoreConfig {
    pub command_bus: CommandBusConfig,
    pub event_bus: EventBusConfig,
}

impl CoreConfig {
    pub fn new() -> Self {
        Self {
            command_bus: CommandBusConfig::new(),
            event_bus: EventBusConfig::default(),
        }
    }
}
