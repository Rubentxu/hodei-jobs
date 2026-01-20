//!
//! # Legacy Migration Support
//!
//! Utilities for migrating from legacy saga implementation to saga-engine v4.0.
//!

pub mod equivalence;
pub mod scanner;
pub mod serializer;

pub use equivalence::{StateEquivalenceVerifier, VerificationResult};
pub use scanner::{LegacyCodeScanner, ScanResult, ScanTarget};
pub use serializer::{BatchSagaStateSerializer, LegacySagaStateSerializer, SerializerError};
