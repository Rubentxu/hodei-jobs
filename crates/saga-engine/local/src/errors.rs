//! # Local Errors Module
//!
//! This module provides error types for local saga engine adapters.
//! Includes wrapper types for creating type-safe trait objects.

use std::convert::Infallible;
use std::fmt::Debug;

/// A wrapper that converts any error to `Infallible` for local-only implementations.
///
/// This type implements `Debug + Send + Sync + 'static` which are required
/// by the trait object bounds, while representing the fact that local
/// implementations should never produce errors in normal operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NeverError(Infallible);

impl NeverError {
    /// Create a new `NeverError`. This will always panic if the error is accessed.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self(panic!(
            "NeverError was accessed - this should never happen in local mode"
        ))
    }

    /// Unwrap the inner error. This will always panic.
    #[inline]
    pub fn into_inner(self) -> Infallible {
        panic!("NeverError was accessed - this should never happen in local mode")
    }
}

impl Default for NeverError {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NeverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "never error")
    }
}

impl std::error::Error for NeverError {}

/// Convert any error type to `NeverError` for local-only usage.
///
/// This function is used to satisfy trait object bounds where an error type
/// is required but local implementations don't produce errors.
#[inline]
pub fn never<T>(_: impl std::fmt::Debug) -> T {
    panic!("Operation should never fail in local mode")
}

/// Adapter that wraps a local implementation to satisfy the Error type bound.
///
/// This allows using local implementations (which never fail) as trait objects
/// that require `Error: Debug + Send + Sync + 'static`.
pub struct LocalErrorAdapter<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T> LocalErrorAdapter<T> {
    /// Create a new local error adapter.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: Debug + Send + Sync + 'static> From<T> for LocalErrorAdapter<T> {
    #[inline]
    fn from(_: T) -> Self {
        Self::new()
    }
}

impl<T> Default for LocalErrorAdapter<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
