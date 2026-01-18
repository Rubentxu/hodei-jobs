//! Cached Saga Repository
//!
//! Wrapper that adds LRU caching with TTL to saga repository operations.
//! Reduces database roundtrips for frequently accessed saga data.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::debug;

use crate::saga::{
    SagaContext, SagaId, SagaRepository, SagaState, SagaStepData, SagaStepId, SagaStepState,
    SagaType,
};

/// LRU Cache with TTL (using RwLock for async compatibility)
#[derive(Clone)]
struct TtlCache<K: Clone + std::hash::Hash + Eq, V> {
    entries: Arc<RwLock<Vec<CacheEntry<K, V>>>>,
    capacity: usize,
    ttl: Duration,
}

struct CacheEntry<K, V> {
    key: K,
    value: V,
    cached_at: Instant,
}

impl<K: Clone + std::hash::Hash + Eq + std::cmp::Ord> TtlCache<K, Vec<SagaStepData>> {
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            capacity,
            ttl,
        }
    }

    fn get(&self, key: &K) -> Option<Vec<SagaStepData>> {
        let entries = self.entries.read().unwrap();
        let now = Instant::now();
        entries
            .iter()
            .find(|e| e.key == *key && now.duration_since(e.cached_at) < self.ttl)
            .map(|e| e.value.clone())
    }

    fn insert(&self, key: K, value: Vec<SagaStepData>) {
        let mut entries = self.entries.write().unwrap();
        let now = Instant::now();

        // Remove old entries for this key
        entries.retain(|e| e.key != key);

        // Add new entry
        entries.push(CacheEntry {
            key,
            value,
            cached_at: now,
        });

        // Enforce capacity limit
        while entries.len() > self.capacity {
            entries.remove(0);
        }
    }

    fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.clear();
    }

    fn len(&self) -> usize {
        let entries = self.entries.read().unwrap();
        entries.len()
    }
}

/// Cached Saga Repository
///
/// Wraps an existing SagaRepository with an LRU cache that has TTL expiration.
/// Cache hits bypass the database entirely, reducing latency and load.
#[derive(Clone)]
pub struct CachedSagaRepository<R: SagaRepository> {
    inner: Arc<R>,
    steps_cache: TtlCache<SagaId, Vec<SagaStepData>>,
    ttl: Duration,
    cache_capacity: usize,
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub capacity: usize,
    pub ttl_seconds: u64,
    pub items: usize,
}

impl<R: SagaRepository> CachedSagaRepository<R> {
    /// Create a new cached repository with default settings
    pub fn new(inner: Arc<R>) -> Self {
        Self::with_config(inner, 1000, Duration::from_secs(5))
    }

    /// Create a new cached repository with custom configuration
    pub fn with_config(inner: Arc<R>, cache_capacity: usize, ttl: Duration) -> Self {
        Self {
            inner,
            steps_cache: TtlCache::new(cache_capacity, ttl),
            ttl,
            cache_capacity,
        }
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.steps_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            capacity: self.cache_capacity,
            ttl_seconds: self.ttl.as_secs(),
            items: self.steps_cache.len(),
        }
    }
}

#[async_trait]
impl<R: SagaRepository> SagaRepository for CachedSagaRepository<R> {
    type Error = R::Error;

    async fn save(&self, context: &SagaContext) -> Result<(), Self::Error> {
        self.inner.save(context).await?;
        self.steps_cache.clear();
        Ok(())
    }

    async fn create_if_not_exists(&self, context: &SagaContext) -> Result<bool, Self::Error> {
        let result = self.inner.create_if_not_exists(context).await?;
        if result {
            self.steps_cache.clear();
        }
        Ok(result)
    }

    async fn find_by_id(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
        // No caching for saga context - always fetch fresh
        self.inner.find_by_id(saga_id).await
    }

    async fn find_by_type(&self, saga_type: SagaType) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner.find_by_type(saga_type).await
    }

    async fn find_by_state(&self, state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner.find_by_state(state).await
    }

    async fn find_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner.find_by_correlation_id(correlation_id).await
    }

    async fn update_state(
        &self,
        saga_id: &SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error> {
        self.inner
            .update_state(saga_id, state, error_message)
            .await?;
        self.steps_cache.clear();
        Ok(())
    }

    async fn mark_compensating(&self, saga_id: &SagaId) -> Result<(), Self::Error> {
        self.inner.mark_compensating(saga_id).await?;
        self.steps_cache.clear();
        Ok(())
    }

    async fn delete(&self, saga_id: &SagaId) -> Result<bool, Self::Error> {
        let result = self.inner.delete(saga_id).await?;
        self.steps_cache.clear();
        Ok(result)
    }

    async fn save_step(&self, step: &SagaStepData) -> Result<(), Self::Error> {
        self.inner.save_step(step).await?;
        self.steps_cache.clear();
        Ok(())
    }

    async fn find_step_by_id(
        &self,
        step_id: &SagaStepId,
    ) -> Result<Option<SagaStepData>, Self::Error> {
        self.inner.find_step_by_id(step_id).await
    }

    async fn find_steps_by_saga_id(
        &self,
        saga_id: &SagaId,
    ) -> Result<Vec<SagaStepData>, Self::Error> {
        // Try cache first
        if let Some(steps) = self.steps_cache.get(saga_id) {
            return Ok(steps);
        }

        // Cache miss - fetch from DB
        let steps = self.inner.find_steps_by_saga_id(saga_id).await?;

        // Cache the result
        self.steps_cache.insert(saga_id.clone(), steps.clone());

        Ok(steps)
    }

    async fn update_step_state(
        &self,
        step_id: &SagaStepId,
        state: SagaStepState,
        output: Option<serde_json::Value>,
    ) -> Result<(), Self::Error> {
        self.inner.update_step_state(step_id, state, output).await?;
        self.steps_cache.clear();
        Ok(())
    }

    async fn update_step_compensation(
        &self,
        step_id: &SagaStepId,
        compensation_data: serde_json::Value,
    ) -> Result<(), Self::Error> {
        self.inner
            .update_step_compensation(step_id, compensation_data)
            .await?;
        self.steps_cache.clear();
        Ok(())
    }

    async fn count_active(&self) -> Result<u64, Self::Error> {
        self.inner.count_active().await
    }

    async fn count_by_type_and_state(
        &self,
        saga_type: SagaType,
        state: SagaState,
    ) -> Result<u64, Self::Error> {
        self.inner.count_by_type_and_state(saga_type, state).await
    }

    async fn avg_duration(&self) -> Result<Option<Duration>, Self::Error> {
        self.inner.avg_duration().await
    }

    async fn cleanup_completed(&self, older_than: Duration) -> Result<u64, Self::Error> {
        let result = self.inner.cleanup_completed(older_than).await?;
        self.steps_cache.clear();
        Ok(result)
    }

    async fn claim_pending_sagas(
        &self,
        limit: u64,
        instance_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner.claim_pending_sagas(limit, instance_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::{SagaContext, SagaId, SagaType};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    // Mock repository for testing
    struct MockSagaRepository {
        sagas: Mutex<Vec<SagaContext>>,
    }

    impl MockSagaRepository {
        fn new() -> Self {
            Self {
                sagas: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl SagaRepository for MockSagaRepository {
        type Error = std::io::Error;

        async fn save(&self, context: &SagaContext) -> Result<(), Self::Error> {
            let mut sagas = self.sagas.lock().unwrap();
            sagas.push(context.clone());
            Ok(())
        }

        async fn create_if_not_exists(&self, context: &SagaContext) -> Result<bool, Self::Error> {
            let mut sagas = self.sagas.lock().unwrap();
            if sagas.iter().any(|s| s.saga_id == context.saga_id) {
                return Ok(false);
            }
            sagas.push(context.clone());
            Ok(true)
        }

        async fn find_by_id(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
            let sagas = self.sagas.lock().unwrap();
            Ok(sagas.iter().find(|s| s.saga_id == *saga_id).cloned())
        }

        async fn find_by_type(
            &self,
            _saga_type: SagaType,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(self.sagas.lock().unwrap().clone())
        }

        async fn find_by_state(&self, _state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(self.sagas.lock().unwrap().clone())
        }

        async fn find_by_correlation_id(
            &self,
            _correlation_id: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(self.sagas.lock().unwrap().clone())
        }

        async fn update_state(
            &self,
            _saga_id: &SagaId,
            _state: SagaState,
            _error_message: Option<String>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn mark_compensating(&self, _saga_id: &SagaId) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn delete(&self, _saga_id: &SagaId) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn save_step(&self, _step: &SagaStepData) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn find_step_by_id(
            &self,
            _step_id: &SagaStepId,
        ) -> Result<Option<SagaStepData>, Self::Error> {
            Ok(None)
        }

        async fn find_steps_by_saga_id(
            &self,
            _saga_id: &SagaId,
        ) -> Result<Vec<SagaStepData>, Self::Error> {
            Ok(vec![])
        }

        async fn update_step_state(
            &self,
            _step_id: &SagaStepId,
            _state: SagaStepState,
            _output: Option<serde_json::Value>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn update_step_compensation(
            &self,
            _step_id: &SagaStepId,
            _compensation_data: serde_json::Value,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn count_active(&self) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn count_by_type_and_state(
            &self,
            _saga_type: SagaType,
            _state: SagaState,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn avg_duration(&self) -> Result<Option<Duration>, Self::Error> {
            Ok(None)
        }

        async fn cleanup_completed(&self, _older_than: Duration) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn claim_pending_sagas(
            &self,
            _limit: u64,
            _instance_id: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_steps_cache_hit() {
        let inner = Arc::new(MockSagaRepository::new());
        let cached = CachedSagaRepository::with_config(inner.clone(), 100, Duration::from_secs(5));

        let saga_id = SagaId::new();

        // First call - cache miss
        let steps1 = cached.find_steps_by_saga_id(&saga_id).await.unwrap();
        assert!(steps1.is_empty());

        // Second call - cache hit (empty vec cached)
        let steps2 = cached.find_steps_by_saga_id(&saga_id).await.unwrap();
        assert!(steps2.is_empty());

        // Check cache stats
        let stats = cached.cache_stats();
        assert_eq!(stats.items, 1);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let inner = Arc::new(MockSagaRepository::new());
        let cached = CachedSagaRepository::new(inner.clone());

        let saga_id = SagaId::new();
        cached.find_steps_by_saga_id(&saga_id).await.unwrap();

        assert_eq!(cached.cache_stats().items, 1);

        // Clear cache
        cached.clear_cache();

        assert_eq!(cached.cache_stats().items, 0);
    }
}
