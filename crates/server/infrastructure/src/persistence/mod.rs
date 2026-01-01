// Persistence Layer - Implementaciones de repositorios por tecnología

pub mod outbox;
pub mod postgres;

pub use postgres::*;

// Re-export MigrationConfig for convenience
pub use postgres::MigrationConfig;
