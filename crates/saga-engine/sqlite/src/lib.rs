//! # saga-engine-sqlite
//!
//! SQLite backend implementation for saga-engine.
//!
//! This crate provides SQLite implementations for local applications:
//! - [`SqliteEventStore`] - Event persistence with ACID transactions
//! - [`SqliteTimerStore`] - In-memory timer storage (with SQLite persistence optional)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │           Application Layer                          │
//! │        LocalSagaEngine (Facade)                      │
//! └──────────────────────┬──────────────────────────────┘
//!                        │ uses adapters
//! ┌──────────────────────▼──────────────────────────────┐
//! │           Infrastructure Layer                       │
//! │  SqliteEventStore   │  SqliteTimerStore (Adapter)  │
//! └──────────────────────┴──────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use saga_engine_sqlite::{SqliteEventStore, SqliteEventStoreConfig};
//!
//! // Create event store with file-based persistence
//! let event_store = SqliteEventStore::new("/tmp/my-app.db").await?;
//!
//! // Or use in-memory for testing
//! let event_store = SqliteEventStore::in_memory().await?;
//!
//! // Use with custom configuration
//! let event_store = SqliteEventStore::builder()
//!     .path("/tmp/my-app.db")
//!     .snapshot_every(100)
//!     .build()
//!     .await?;
//! ```

pub mod event_store;
pub mod timer_store;

pub use event_store::{SqliteEventStore, SqliteEventStoreConfig, SqliteEventStoreError};
pub use timer_store::{SqliteTimerStore, SqliteTimerStoreConfig, SqliteTimerStoreError};

#[cfg(test)]
mod tests {
    use saga_engine_core::event::{EventCategory, EventType, HistoryEvent};
    use serde_json::json;

    fn make_test_event(saga_id: &saga_engine_core::event::SagaId, event_num: u64) -> HistoryEvent {
        HistoryEvent::builder()
            .event_id(saga_engine_core::event::EventId(event_num))
            .saga_id(saga_id.clone())
            .event_type(EventType::WorkflowExecutionStarted)
            .category(EventCategory::Workflow)
            .payload(json!({"event_num": event_num}))
            .build()
    }
}
