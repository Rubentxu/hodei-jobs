// Persistence Layer - Implementaciones de repositorios por tecnología

// TEMPORARILY DISABLED - Outbox module has compilation errors
// pub mod outbox;
pub mod postgres;

pub use postgres::*;
