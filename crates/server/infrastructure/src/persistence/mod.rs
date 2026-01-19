// Persistence Layer - Implementaciones de repositorios por tecnología

pub mod command_outbox;
pub mod outbox;
pub mod outbox_cleanup;
pub mod postgres;
pub mod postgres_idempotency;
pub mod saga;

pub use outbox_cleanup::{OutboxCleanupConfig, OutboxCleanupMetrics, start_outbox_cleanup_worker};
pub use postgres::*;
pub use postgres_idempotency::PostgresIdempotencyChecker;
pub use saga::*;

// Re-export MigrationConfig for convenience
pub use postgres::MigrationConfig;
