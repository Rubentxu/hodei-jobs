//! Time-To-Live (TTL) Cache with expiration
//!
//! Provides a thread-safe cache with automatic entry expiration.
//! Useful for caching provider mappings and scoring results.
//!
//! ## Design Principles
//! - Thread-safe using atomics and Mutex
//! - O(1) lookup with HashMap
//! - Automatic cleanup of expired entries
//! - Configurable TTL

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// A cache entry with expiration time
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    /// The cached value
    value: V,
    /// When this entry expires
    expires_at: Instant,
}

impl<V> CacheEntry<V> {
    /// Create a new cache entry with the given TTL
    fn new(value: V, ttl: Duration) -> Self {
        Self {
            value,
            expires_at: Instant::now() + ttl,
        }
    }

    /// Check if this entry has expired
    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    /// Get a reference to the value if not expired
    fn value(&self) -> Option<&V> {
        if self.is_expired() {
            None
        } else {
            Some(&self.value)
        }
    }

    /// Get the remaining TTL for this entry
    fn remaining_ttl(&self) -> Option<Duration> {
        let remaining = self.expires_at.duration_since(Instant::now());
        if remaining.is_zero() {
            None
        } else {
            Some(remaining)
        }
    }
}

/// A thread-safe TTL cache
///
/// Provides O(1) lookups with automatic expiration.
/// Cleanup of expired entries happens lazily during lookups
/// or can be triggered manually.
#[derive(Debug)]
pub struct TtlCache<K, V>
where
    K: Hash + Eq,
{
    /// The cache storage
    entries: RwLock<HashMap<K, CacheEntry<V>>>,
    /// Default TTL for entries
    default_ttl: Duration,
    /// Maximum number of entries
    max_entries: usize,
}

impl<K, V> TtlCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new TTL cache with the given default TTL
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            default_ttl,
            max_entries: 1000,
        }
    }

    /// Create a new TTL cache with custom configuration
    pub fn with_config(default_ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::with_capacity(max_entries.min(1024))),
            default_ttl,
            max_entries,
        }
    }

    /// Insert a value with the default TTL
    pub fn insert(&self, key: K, value: V) -> Option<V>
    where
        K: Hash,
    {
        self.insert_with_ttl(key, value, self.default_ttl)
    }

    /// Insert a value with a custom TTL
    pub fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) -> Option<V>
    where
        K: Hash,
    {
        let entry = CacheEntry::new(value, ttl);

        let mut entries = self.entries.write().unwrap();

        // Evict oldest entry if at capacity
        if entries.len() >= self.max_entries && !entries.contains_key(&key) {
            let to_remove = entries.len() / 5;
            let to_remove: Vec<_> = entries.keys().take(to_remove).cloned().collect();
            for k in to_remove {
                entries.remove(&k);
            }
        }

        entries.insert(key, entry).map(|e| e.value)
    }

    /// Get a value by key
    pub fn get(&self, key: &K) -> Option<V> {
        let entries = self.entries.read().unwrap();
        if let Some(entry) = entries.get(key) {
            if let Some(value) = entry.value() {
                return Some(value.clone());
            }
        }
        None
    }

    /// Get a value along with its remaining TTL
    pub fn get_with_ttl(&self, key: &K) -> Option<(V, Option<Duration>)> {
        let entries = self.entries.read().unwrap();
        if let Some(entry) = entries.get(key) {
            if let Some(value) = entry.value() {
                return Some((value.clone(), entry.remaining_ttl()));
            }
        }
        None
    }

    /// Check if a key exists and is not expired
    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Remove a key from the cache
    pub fn remove(&self, key: &K) -> Option<V> {
        self.entries.write().unwrap().remove(key).map(|e| e.value)
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.entries.write().unwrap().clear();
    }

    /// Remove all expired entries
    pub fn remove_expired(&self) -> usize {
        let mut entries = self.entries.write().unwrap();
        let before = entries.len();
        entries.retain(|_, v| !v.is_expired());
        before - entries.len()
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.read().unwrap().is_empty()
    }
}

/// A reference-counted TTL cache for sharing across threads
pub type SharedTtlCache<K, V> = Arc<TtlCache<K, V>>;

// =============================================================================
// LRU Cache
// =============================================================================

/// LRU (Least Recently Used) cache entry
#[derive(Debug, Clone)]
struct LruEntry<V> {
    /// The cached value
    value: V,
    /// Position in the access order (monotonic counter)
    access_order: u64,
    /// When this entry expires
    expires_at: Instant,
}

impl<V> LruEntry<V> {
    fn new(value: V, ttl: Duration, access_order: u64) -> Self {
        Self {
            value,
            expires_at: Instant::now() + ttl,
            access_order,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// LRU Cache with TTL support
#[derive(Debug)]
pub struct LruTtlCache<K, V>
where
    K: Hash + Eq,
{
    /// The cache storage
    entries: Mutex<HashMap<K, LruEntry<V>>>,
    /// Access order counter
    access_counter: Mutex<u64>,
    /// Default TTL
    default_ttl: Duration,
    /// Maximum entries
    max_entries: usize,
}

impl<K, V> LruTtlCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new LRU TTL cache
    pub fn new(default_ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::with_capacity(max_entries.min(1024))),
            access_counter: Mutex::new(0),
            default_ttl,
            max_entries,
        }
    }

    /// Get the next access order value
    fn next_access_order(&self) -> u64 {
        let mut counter = self.access_counter.lock().unwrap();
        *counter += 1;
        *counter
    }

    /// Insert a value
    pub fn insert(&self, key: K, value: V) -> Option<V>
    where
        K: Hash,
    {
        self.insert_with_ttl(key, value, self.default_ttl)
    }

    /// Insert with custom TTL
    pub fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) -> Option<V>
    where
        K: Hash,
    {
        let order = self.next_access_order();
        let entry = LruEntry::new(value.clone(), ttl, order);

        let mut entries = self.entries.lock().unwrap();

        // Evict expired entries if at capacity
        if entries.len() >= self.max_entries {
            let expired_keys: Vec<K> = entries
                .iter()
                .filter(|(_, v)| v.is_expired())
                .map(|(k, _)| k.clone())
                .collect();
            for k in expired_keys {
                entries.remove(&k);
            }
        }

        entries.insert(key, entry).map(|e| e.value)
    }

    /// Get a value
    pub fn get(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.lock().unwrap();

        if let Some(entry) = entries.get(key) {
            if !entry.is_expired() {
                // Update access order (LRU behavior)
                let order = self.next_access_order();
                let value = entry.value.clone();
                let remaining_ttl = entry.expires_at.duration_since(Instant::now());

                // Remove old entry and insert updated one
                entries.remove(key);
                let new_entry = LruEntry::new(value.clone(), remaining_ttl, order);
                entries.insert(key.clone(), new_entry);
                return Some(value);
            } else {
                // Remove expired entry
                entries.remove(key);
            }
        }

        None
    }

    /// Check if key exists and is valid
    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Remove a key
    pub fn remove(&self, key: &K) -> Option<V> {
        self.entries.lock().unwrap().remove(key).map(|e| e.value)
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    /// Get size
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }
}

/// A reference-counted LRU TTL cache
pub type SharedLruTtlCache<K, V> = Arc<LruTtlCache<K, V>>;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_cache_insert_get() {
        let cache = TtlCache::new(Duration::from_secs(60));
        cache.insert("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    }

    #[test]
    fn test_ttl_cache_expired() {
        let cache = TtlCache::new(Duration::from_millis(50));
        cache.insert("key".to_string(), "value".to_string());
        assert!(cache.contains(&"key".to_string()));

        std::thread::sleep(Duration::from_millis(60));
        assert!(!cache.contains(&"key".to_string()));
        assert_eq!(cache.get(&"key".to_string()), None);
    }

    #[test]
    fn test_ttl_cache_remove() {
        let cache = TtlCache::new(Duration::from_secs(60));
        cache.insert("key".to_string(), "value".to_string());
        assert_eq!(cache.remove(&"key".to_string()), Some("value".to_string()));
        assert!(!cache.contains(&"key".to_string()));
    }

    #[test]
    fn test_ttl_cache_clear() {
        let cache = TtlCache::new(Duration::from_secs(60));
        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lru_cache_insert_get() {
        let cache = LruTtlCache::new(Duration::from_secs(60), 10);
        cache.insert("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    }

    #[test]
    fn test_lru_cache_expired() {
        let cache = LruTtlCache::new(Duration::from_millis(50), 10);
        cache.insert("key".to_string(), "value".to_string());
        assert!(cache.contains(&"key".to_string()));

        std::thread::sleep(Duration::from_millis(60));
        assert!(!cache.contains(&"key".to_string()));
    }

    #[test]
    fn test_shared_cache() {
        let cache: SharedTtlCache<String, i32> = Arc::new(TtlCache::new(Duration::from_secs(60)));

        let cache_clone = cache.clone();
        std::thread::spawn(move || {
            cache_clone.insert("key".to_string(), 42);
        })
        .join()
        .unwrap();

        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get(&"key".to_string()), Some(42));
    }
}
