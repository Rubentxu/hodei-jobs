// Hodei Job Platform - Infrastructure Layer
// Implementaciones concretas de repositorios, clientes externos y persistencia
// Módulos:
// - repositories: Implementaciones de repositorios
// - providers: Implementaciones de WorkerProvider (Docker, K8s, etc.)
// - persistence: Adaptadores de persistencia (DB, archivos)

pub mod repositories;
pub mod providers;
pub mod persistence;

// Legacy module - will be replaced by providers
pub mod external_clients;

#[cfg(test)]
mod tests;

pub use repositories::*;
pub use providers::*;
pub use persistence::*;