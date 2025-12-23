// Persistence Layer - Implementaciones de repositorios por tecnología

pub mod outbox;
pub mod postgres;

pub use postgres::*;
