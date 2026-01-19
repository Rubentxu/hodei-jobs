//! Saga Adapters Module
//!
//! This module provides adapters for migrating from the legacy saga
//! implementation to the saga-engine v4.0 library.
//!
//! Note: legacy_adapter and v4_adapter must be declared before factory
//! since factory depends on them.

pub mod factory;
pub mod legacy_adapter;
pub mod v4_adapter;

pub use legacy_adapter::LegacySagaAdapter;
