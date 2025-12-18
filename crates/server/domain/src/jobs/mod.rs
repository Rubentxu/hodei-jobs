//! Jobs Bounded Context
//!
//! Maneja el lifecycle de jobs, especificaciones, plantillas y coordinación

pub mod aggregate;
pub mod coordination;
pub mod templates;

pub use aggregate::*;
pub use coordination::*;
pub use templates::*;
