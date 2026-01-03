// Persistence Layer - Implementaciones de repositorios por tecnología

pub mod outbox;
pub mod postgres;
pub mod saga;

pub use postgres::*;
pub use saga::*;

// Re-export MigrationConfig for convenience
pub use postgres::MigrationConfig;
