//! Hybrid Outbox Module - Shared components for Command and Event outboxes
//!
//! This module provides shared infrastructure for both Command and Event outboxes,
//! implementing the hybrid LISTEN/NOTIFY + polling pattern.
//!
//! # Components
//!
//! - [`PgNotifyListener`]: PostgreSQL LISTEN/NOTIFY wrapper
//! - [`BackoffConfig`]: Exponential backoff configuration
//! - [`HybridOutboxRelay`]: Generic relay implementation
//!
//! # Usage
//!
//! ```rust
//! use hodei_server_infrastructure::messaging::hybrid::{
//!     PgNotifyListener,
//!     BackoffConfig,
//!     HybridOutboxRelay,
//! };
//!
//! // Create a listener for commands
//! let listener = PgNotifyListener::for_commands(&pool).await?;
//!
//! // Use standard backoff configuration
//! let backoff = BackoffConfig::standard();
//!
//! // Create hybrid relay
//! let relay = HybridOutboxRelay::new(&pool, event_bus, processor, None).await?;
//! ```

pub mod backoff;
pub mod pg_notify_listener;
pub mod relay;

pub use backoff::{BackoffConfig, BackoffStats};
pub use pg_notify_listener::{LoggingNotificationHandler, NullNotificationHandler, PgNotifyListener, PgNotifyListenerBuilder, NotificationHandler};
pub use relay::{EventProcessor, HybridOutboxConfig, HybridOutboxMetrics, HybridOutboxRelay, NatsEventProcessor};
