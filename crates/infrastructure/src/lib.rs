// Hodei Job Platform - Infrastructure Layer
// Implementaciones concretas de repositorios, clientes externos y persistencia
// Módulos:
// - repositories: Implementaciones de repositorios
// - providers: Implementaciones de WorkerProvider (Docker, K8s, etc.)
// - persistence: Adaptadores de persistencia (DB, archivos)

pub mod event_bus;
pub mod persistence;
pub mod providers;
pub mod repositories;

// Legacy module - will be replaced by providers
pub mod external_clients;

#[cfg(test)]
mod tests;

pub use persistence::*;
pub use providers::*;
pub use repositories::*;
