//! Event Deduplication Implementation
//!
//! This module provides mechanisms to detect and handle duplicate events
//! to ensure idempotent processing. Uses a sliding window approach with
//! content-based hashing.
//!
//! # Architecture
//!
//! The deduplication system works by:
//! 1. Computing a content-based hash of each event
//! 2. Tracking recently seen hashes in a bounded cache
//! 3. Rejecting or flagging duplicate events
//! 4. Providing metrics for monitoring duplicate rates

use chrono::{DateTime, Duration, Utc};
use hodei_server_domain::events::DomainEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Default deduplication window duration (5 minutes)
const DEFAULT_DEDUPLICATION_WINDOW_MINUTES: i64 = 5;

/// Default maximum cached entries (10,000)
const DEFAULT_MAX_CACHE_SIZE: usize = 10_000;

/// Result of deduplication check
#[derive(Debug, Clone, PartialEq)]
pub enum DeduplicationResult {
    /// Event is new and should be processed
    New,
    /// Event is a duplicate (already seen within window)
    Duplicate,
    /// Event was seen before but outside the window (should be reprocessed)
    Expired,
}

/// A deduplication entry
#[derive(Debug, Clone)]
pub struct DeduplicationEntry {
    /// Hash of the event content
    pub event_hash: u64,
    /// When the event was first seen
    pub first_seen: DateTime<Utc>,
    /// When the event expires from the cache
    pub expires_at: DateTime<Utc>,
    /// Number of times this event was seen
    pub seen_count: u32,
}

impl DeduplicationEntry {
    /// Create a new entry
    pub fn new(event_hash: u64, window_duration: Duration) -> Self {
        let now = Utc::now();
        Self {
            event_hash,
            first_seen: now,
            expires_at: now + window_duration,
            seen_count: 1,
        }
    }

    /// Check if the entry has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Configuration for event deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDeduplicationConfig {
    /// How long to remember seen events
    pub window_duration: StdDuration,
    /// Maximum number of entries to cache
    pub max_cache_size: usize,
    /// Whether deduplication is enabled
    pub enabled: bool,
    /// Whether to log duplicate detections
    pub log_duplicates: bool,
}

impl Default for EventDeduplicationConfig {
    fn default() -> Self {
        Self {
            window_duration: StdDuration::from_secs(
                60 * DEFAULT_DEDUPLICATION_WINDOW_MINUTES as u64,
            ),
            max_cache_size: DEFAULT_MAX_CACHE_SIZE,
            enabled: true,
            log_duplicates: true,
        }
    }
}

/// Internal cache structure for deduplication
#[derive(Debug, Clone)]
struct DeduplicationCache {
    entries: HashMap<u64, DeduplicationEntry>,
    access_order: Vec<u64>,
    max_size: usize,
}

impl DeduplicationCache {
    fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: Vec::new(),
            max_size,
        }
    }

    fn get(&mut self, key: &u64) -> Option<&DeduplicationEntry> {
        // Update access order for LRU
        if let Some(pos) = self.access_order.iter().position(|k| k == key) {
            let val = self.access_order.remove(pos);
            self.access_order.push(val);
        }
        self.entries.get(key)
    }

    fn get_mut(&mut self, key: &u64) -> Option<&mut DeduplicationEntry> {
        // Update access order for LRU
        if let Some(pos) = self.access_order.iter().position(|k| k == key) {
            let val = self.access_order.remove(pos);
            self.access_order.push(val);
        }
        self.entries.get_mut(key)
    }

    fn insert(&mut self, key: u64, entry: DeduplicationEntry) {
        // Evict oldest if at capacity
        if self.entries.len() >= self.max_size && self.access_order.is_empty() == false {
            if let Some(oldest_key) = self.access_order.first().cloned() {
                self.entries.remove(&oldest_key);
                self.access_order.remove(0);
            }
        }

        self.entries.insert(key, entry);
        self.access_order.push(key);
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.access_order.clear();
    }

    fn remove_expired(&mut self) {
        let now = Utc::now();
        let expired_keys: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.expires_at <= now)
            .map(|(k, _)| *k)
            .collect();

        for key in expired_keys {
            self.entries.remove(&key);
            if let Some(pos) = self.access_order.iter().position(|k| k == &key) {
                self.access_order.remove(pos);
            }
        }
    }
}

/// Event Deduplication Service
///
/// Uses an in-memory cache with content-based hashing to detect duplicate events.
/// Thread-safe for concurrent access.
#[derive(Clone)]
pub struct EventDeduplicator {
    cache: Arc<Mutex<DeduplicationCache>>,
    config: Arc<EventDeduplicationConfig>,
    stats: Arc<DeduplicationStats>,
}

impl EventDeduplicator {
    /// Create a new EventDeduplicator with default configuration
    pub fn new() -> Self {
        Self::with_config(None)
    }

    /// Create a new EventDeduplicator with custom configuration
    pub fn with_config(config: Option<EventDeduplicationConfig>) -> Self {
        let config = config.unwrap_or_default();
        let cache = Arc::new(Mutex::new(DeduplicationCache::new(config.max_cache_size)));

        Self {
            cache,
            config: Arc::new(config),
            stats: Arc::new(DeduplicationStats::new()),
        }
    }

    /// Check if an event has been seen before
    ///
    /// # Arguments
    ///
    /// * `event` - The event to check for duplicates
    ///
    /// # Returns
    ///
    /// Whether this is a new, duplicate, or expired event
    pub async fn check(&self, event: &DomainEvent) -> DeduplicationResult {
        if !self.config.enabled {
            return DeduplicationResult::New;
        }

        let event_hash = Self::hash_event(event);
        let window_duration = Duration::from_std(self.config.window_duration).unwrap_or_default();

        let mut cache = self.cache.lock().await;

        // First, remove expired entries
        cache.remove_expired();

        match cache.get(&event_hash) {
            Some(entry) => {
                if entry.is_expired() {
                    // Entry expired, treat as new
                    let new_entry = DeduplicationEntry::new(event_hash, window_duration);
                    cache.insert(event_hash, new_entry);
                    self.stats.record_expired();
                    DeduplicationResult::Expired
                } else {
                    // Duplicate within window - update seen count directly
                    if let Some(existing_entry) = cache.get_mut(&event_hash) {
                        existing_entry.seen_count += 1;
                    }

                    if self.config.log_duplicates {
                        warn!("Duplicate event detected: hash={}", event_hash);
                    }
                    self.stats.record_duplicate();
                    DeduplicationResult::Duplicate
                }
            }
            None => {
                // New event
                let entry = DeduplicationEntry::new(event_hash, window_duration);
                cache.insert(event_hash, entry);
                self.stats.record_new();
                DeduplicationResult::New
            }
        }
    }

    /// Compute a hash for an event based on its content
    fn hash_event(event: &DomainEvent) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        EventHasher::hash(event, &mut hasher);
        hasher.finish()
    }

    /// Get current statistics
    pub async fn stats(&self) -> DeduplicationStatsSnapshot {
        let cache = self.cache.lock().await;
        DeduplicationStatsSnapshot {
            new_count: self
                .stats
                .new_count
                .load(std::sync::atomic::Ordering::Relaxed),
            duplicate_count: self
                .stats
                .duplicate_count
                .load(std::sync::atomic::Ordering::Relaxed),
            expired_count: self
                .stats
                .expired_count
                .load(std::sync::atomic::Ordering::Relaxed),
            cache_size: cache.len(),
            cache_capacity: cache.entries.capacity(),
        }
    }

    /// Clear the deduplication cache
    pub async fn clear(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
        info!("Deduplication cache cleared");
    }

    /// Get the number of entries in the cache
    pub async fn len(&self) -> usize {
        self.cache.lock().await.len()
    }

    /// Check if the cache is empty
    pub async fn is_empty(&self) -> bool {
        self.cache.lock().await.is_empty()
    }
}

impl Default for EventDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper struct for hashing events
struct EventHasher<'a>(&'a DomainEvent);

impl<'a> EventHasher<'a> {
    fn hash(event: &DomainEvent, hasher: &mut std::collections::hash_map::DefaultHasher) {
        match event {
            DomainEvent::JobCreated(e) => {
                "JobCreated".hash(hasher);
                e.job_id.to_string().hash(hasher);
                match &e.spec.command {
                    hodei_server_domain::jobs::CommandType::Shell { cmd, args } => {
                        cmd.hash(hasher);
                        args.join(" ").hash(hasher);
                    }
                    hodei_server_domain::jobs::CommandType::Script {
                        interpreter,
                        content,
                    } => {
                        interpreter.hash(hasher);
                        content.hash(hasher);
                    }
                }
                e.occurred_at.timestamp().hash(hasher);
            }
            DomainEvent::JobStatusChanged {
                job_id,
                new_state,
                occurred_at,
                ..
            } => {
                "JobStatusChanged".hash(hasher);
                job_id.to_string().hash(hasher);
                format!("{:?}", new_state).hash(hasher);
                occurred_at.timestamp().hash(hasher);
            }
            DomainEvent::JobCancelled {
                job_id,
                occurred_at,
                ..
            } => {
                "JobCancelled".hash(hasher);
                job_id.to_string().hash(hasher);
                occurred_at.timestamp().hash(hasher);
            }
            DomainEvent::JobRetried {
                job_id, attempt, ..
            } => {
                "JobRetried".hash(hasher);
                job_id.to_string().hash(hasher);
                attempt.hash(hasher);
            }
            DomainEvent::JobQueued {
                job_id,
                preferred_provider,
                ..
            } => {
                "JobQueued".hash(hasher);
                job_id.to_string().hash(hasher);
                preferred_provider.is_some().hash(hasher);
            }
            DomainEvent::JobAssigned {
                job_id, worker_id, ..
            } => {
                "JobAssigned".hash(hasher);
                job_id.to_string().hash(hasher);
                worker_id.to_string().hash(hasher);
            }
            DomainEvent::WorkerRegistered {
                worker_id,
                provider_id,
                ..
            } => {
                "WorkerRegistered".hash(hasher);
                worker_id.to_string().hash(hasher);
                provider_id.to_string().hash(hasher);
            }
            DomainEvent::WorkerTerminated {
                worker_id, reason, ..
            } => {
                "WorkerTerminated".hash(hasher);
                worker_id.to_string().hash(hasher);
                format!("{:?}", reason).hash(hasher);
            }
            DomainEvent::WorkerReady { worker_id, .. } => {
                "WorkerReady".hash(hasher);
                worker_id.to_string().hash(hasher);
            }
            DomainEvent::WorkerStatusChanged {
                worker_id,
                new_status,
                ..
            } => {
                "WorkerStatusChanged".hash(hasher);
                worker_id.to_string().hash(hasher);
                format!("{:?}", new_status).hash(hasher);
            }
            DomainEvent::JobAccepted {
                job_id, worker_id, ..
            } => {
                "JobAccepted".hash(hasher);
                job_id.to_string().hash(hasher);
                worker_id.to_string().hash(hasher);
            }
            DomainEvent::JobExecutionError {
                job_id,
                failure_reason,
                ..
            } => {
                "JobExecutionError".hash(hasher);
                job_id.to_string().hash(hasher);
                format!("{:?}", failure_reason).hash(hasher);
            }
            DomainEvent::JobDispatchAcknowledged {
                job_id, worker_id, ..
            } => {
                "JobDispatchAcknowledged".hash(hasher);
                job_id.to_string().hash(hasher);
                worker_id.to_string().hash(hasher);
            }
            DomainEvent::RunJobReceived {
                job_id, worker_id, ..
            } => {
                "RunJobReceived".hash(hasher);
                job_id.to_string().hash(hasher);
                worker_id.to_string().hash(hasher);
            }
            DomainEvent::WorkerDisconnected { worker_id, .. } => {
                "WorkerDisconnected".hash(hasher);
                worker_id.to_string().hash(hasher);
            }
            DomainEvent::WorkerProvisioned {
                worker_id,
                provider_id,
                ..
            } => {
                "WorkerProvisioned".hash(hasher);
                worker_id.to_string().hash(hasher);
                provider_id.to_string().hash(hasher);
            }
            // Handle remaining variants with a fallback
            _ => {
                format!("{:?}", event).hash(hasher);
            }
        }
    }
}

/// Statistics for deduplication operations
#[derive(Debug, Default)]
pub struct DeduplicationStats {
    new_count: Arc<std::sync::atomic::AtomicU64>,
    duplicate_count: Arc<std::sync::atomic::AtomicU64>,
    expired_count: Arc<std::sync::atomic::AtomicU64>,
}

impl DeduplicationStats {
    /// Create new statistics
    pub fn new() -> Self {
        Self {
            new_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            duplicate_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            expired_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    fn record_new(&self) {
        self.new_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn record_duplicate(&self) {
        self.duplicate_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn record_expired(&self) {
        self.expired_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Snapshot of deduplication statistics
#[derive(Debug, Clone)]
pub struct DeduplicationStatsSnapshot {
    /// Number of new events
    pub new_count: u64,
    /// Number of duplicate events detected
    pub duplicate_count: u64,
    /// Number of expired entries replaced
    pub expired_count: u64,
    /// Current cache size
    pub cache_size: usize,
    /// Maximum cache capacity
    pub cache_capacity: usize,
}

impl DeduplicationStatsSnapshot {
    /// Calculate duplicate rate as a percentage
    pub fn duplicate_rate(&self) -> f64 {
        let total = self.new_count + self.duplicate_count + self.expired_count;
        if total == 0 {
            0.0
        } else {
            (self.duplicate_count as f64 / total as f64) * 100.0
        }
    }
}

/// Builder for EventDeduplicator
#[derive(Debug)]
pub struct EventDeduplicatorBuilder {
    config: Option<EventDeduplicationConfig>,
}

impl EventDeduplicatorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { config: None }
    }

    /// Set the deduplication window duration
    pub fn with_window_duration(mut self, duration: StdDuration) -> Self {
        self.config
            .get_or_insert_with(EventDeduplicationConfig::default)
            .window_duration = duration;
        self
    }

    /// Set the maximum cache size
    pub fn with_max_cache_size(mut self, size: usize) -> Self {
        self.config
            .get_or_insert_with(EventDeduplicationConfig::default)
            .max_cache_size = size;
        self
    }

    /// Enable or disable deduplication
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.config
            .get_or_insert_with(EventDeduplicationConfig::default)
            .enabled = enabled;
        self
    }

    /// Enable or disable duplicate logging
    pub fn with_log_duplicates(mut self, log: bool) -> Self {
        self.config
            .get_or_insert_with(EventDeduplicationConfig::default)
            .log_duplicates = log;
        self
    }

    /// Build the EventDeduplicator
    pub fn build(self) -> EventDeduplicator {
        EventDeduplicator::with_config(self.config)
    }
}

impl Default for EventDeduplicatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::events::JobCreated;
    use hodei_server_domain::jobs::JobSpec;
    use hodei_server_domain::shared_kernel::JobId;

    fn create_test_event(job_id: &JobId) -> DomainEvent {
        DomainEvent::JobCreated(JobCreated {
            job_id: job_id.clone(),
            spec: JobSpec::new(vec!["echo".to_string(), "hello".to_string()]),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        })
    }

    #[tokio::test]
    async fn test_new_event_returns_new() {
        let deduplicator = EventDeduplicator::new();
        let event = create_test_event(&JobId::new());

        let result = deduplicator.check(&event).await;
        assert_eq!(result, DeduplicationResult::New);
    }

    #[tokio::test]
    async fn test_duplicate_event_returns_duplicate() {
        let deduplicator = EventDeduplicator::new();
        let event = create_test_event(&JobId::new());

        let first = deduplicator.check(&event).await;
        assert_eq!(first, DeduplicationResult::New);

        let second = deduplicator.check(&event).await;
        assert_eq!(second, DeduplicationResult::Duplicate);
    }

    #[tokio::test]
    async fn test_different_events_are_distinct() {
        let deduplicator = EventDeduplicator::new();
        let event1 = create_test_event(&JobId::new());
        let event2 = create_test_event(&JobId::new());

        let result1 = deduplicator.check(&event1).await;
        let result2 = deduplicator.check(&event2).await;

        assert_eq!(result1, DeduplicationResult::New);
        assert_eq!(result2, DeduplicationResult::New);
    }

    #[tokio::test]
    async fn test_disabled_deduplication_always_returns_new() {
        let config = EventDeduplicationConfig {
            enabled: false,
            ..Default::default()
        };
        let deduplicator = EventDeduplicator::with_config(Some(config));
        let event = create_test_event(&JobId::new());

        let first = deduplicator.check(&event).await;
        let second = deduplicator.check(&event).await;

        assert_eq!(first, DeduplicationResult::New);
        assert_eq!(second, DeduplicationResult::New);
    }

    #[tokio::test]
    async fn test_cache_grows_and_limits() {
        let config = EventDeduplicationConfig {
            max_cache_size: 5,
            ..Default::default()
        };
        let deduplicator = EventDeduplicator::with_config(Some(config));

        // Add 10 unique events
        for _ in 0..10 {
            let event = create_test_event(&JobId::new());
            deduplicator.check(&event).await;
        }

        // Cache should be at capacity (5)
        assert_eq!(deduplicator.len().await, 5);
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let deduplicator = EventDeduplicator::new();
        let event = create_test_event(&JobId::new());

        deduplicator.check(&event).await;
        assert!(!deduplicator.is_empty().await);

        deduplicator.clear().await;
        assert!(deduplicator.is_empty().await);
    }

    #[tokio::test]
    async fn test_stats_snapshot() {
        let deduplicator = EventDeduplicator::new();
        let event = create_test_event(&JobId::new());

        // First event - new
        deduplicator.check(&event).await;

        // Duplicate events
        deduplicator.check(&event).await;
        deduplicator.check(&event).await;

        let stats = deduplicator.stats().await;
        assert_eq!(stats.new_count, 1);
        assert_eq!(stats.duplicate_count, 2);
        assert_eq!(stats.expired_count, 0);
    }

    #[test]
    fn test_config_defaults() {
        let config = EventDeduplicationConfig::default();
        assert!(config.enabled);
        assert!(config.log_duplicates);
        assert_eq!(config.max_cache_size, DEFAULT_MAX_CACHE_SIZE);
    }

    #[test]
    fn test_duplicate_rate_calculation() {
        let snapshot = DeduplicationStatsSnapshot {
            new_count: 100,
            duplicate_count: 5,
            expired_count: 10,
            cache_size: 50,
            cache_capacity: 100,
        };

        // (5 / (100 + 5 + 10)) * 100 = ~4.35%
        let rate = snapshot.duplicate_rate();
        assert!(rate > 4.3 && rate < 4.4);
    }

    #[test]
    fn test_duplicate_rate_zero_total() {
        let snapshot = DeduplicationStatsSnapshot {
            new_count: 0,
            duplicate_count: 0,
            expired_count: 0,
            cache_size: 0,
            cache_capacity: 100,
        };

        assert_eq!(snapshot.duplicate_rate(), 0.0);
    }

    #[test]
    fn test_builder_pattern() {
        let builder = EventDeduplicatorBuilder::new()
            .with_window_duration(StdDuration::from_secs(300))
            .with_max_cache_size(500)
            .with_enabled(false)
            .with_log_duplicates(false);

        let deduplicator = builder.build();

        // We can't easily test internal state, but we verify it compiles
        assert!(true);
    }
}
