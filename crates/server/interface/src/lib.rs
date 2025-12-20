pub mod grpc;
pub mod http; // Asumiendo que 'api' se renombró o se mantiene como legado
pub mod log_persistence;

// Re-exports
pub use axum;
pub use tonic;
