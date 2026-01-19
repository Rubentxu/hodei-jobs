//! Idempotency Checker Trait
//!
//! This module defines the idempotency checking interface for the domain layer.
//! Implementations are provided by the infrastructure layer (e.g., PostgreSQL).

use async_trait::async_trait;

/// Trait for idempotency checking.
///
/// This trait abstracts the idempotency check mechanism, allowing for
/// both in-memory (testing) and persistent (production) implementations.
///
/// # Example
///
/// ```ignore
/// use hodei_server_domain::command::IdempotencyChecker;
///
/// // In tests: use in-memory implementation
/// let checker = InMemoryIdempotencyChecker::new();
///
/// // In production: use PostgreSQL implementation
/// let checker = PostgresIdempotencyChecker::new(pool);
/// ```
#[async_trait]
pub trait IdempotencyChecker: Send + Sync {
    /// Check if a command with the given idempotency key has already been processed.
    ///
    /// Returns `true` if the key exists and the command was already processed
    /// (status != 'CANCELLED').
    async fn is_duplicate(&self, key: &str) -> bool;

    /// Mark a command as processed with the given idempotency key.
    ///
    /// After this call, `is_duplicate(key)` should return `true`.
    async fn mark_processed(&self, key: &str);

    /// Clear all idempotency records (for testing purposes only).
    async fn clear(&self);
}

/// In-memory idempotency checker for testing.
#[derive(Debug, Default)]
pub struct InMemoryIdempotencyChecker {
    processed: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl InMemoryIdempotencyChecker {
    /// Create a new in-memory idempotency checker.
    pub fn new() -> Self {
        Self {
            processed: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }
}

#[async_trait]
impl IdempotencyChecker for InMemoryIdempotencyChecker {
    async fn is_duplicate(&self, key: &str) -> bool {
        self.processed.lock().unwrap().contains(key)
    }

    async fn mark_processed(&self, key: &str) {
        self.processed.lock().unwrap().insert(key.to_string());
    }

    async fn clear(&self) {
        self.processed.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_idempotency_new_key() {
        let checker = InMemoryIdempotencyChecker::new();
        assert!(!checker.is_duplicate("key-1").await);
    }

    #[tokio::test]
    async fn test_in_memory_idempotency_duplicate() {
        let checker = InMemoryIdempotencyChecker::new();
        checker.mark_processed("key-1").await;
        assert!(checker.is_duplicate("key-1").await);
    }

    #[tokio::test]
    async fn test_in_memory_idempotency_clear() {
        let checker = InMemoryIdempotencyChecker::new();
        checker.mark_processed("key-1").await;
        assert!(checker.is_duplicate("key-1").await);
        checker.clear().await;
        assert!(!checker.is_duplicate("key-1").await);
    }
}
