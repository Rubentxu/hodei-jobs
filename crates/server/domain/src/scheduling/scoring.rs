//! Provider Scoring Traits and Composite Selector
//!
//! Strategy composition for provider selection with reusable scoring components.
//!
//! ## Design Principles
//! - Extract common scoring logic from existing selectors
//! - Enable composition of multiple scoring criteria
//! - Reduce code duplication across selectors
//! - Provide extensible scoring framework

use crate::jobs::Job;
use crate::shared_kernel::ProviderId;
use crate::workers::ProviderType;
use std::time::Duration;

/// Information about a provider used for scoring
///
/// This is a simplified view of ProviderInfo optimized for scoring calculations.
#[derive(Debug, Clone)]
pub struct ProviderScoreInput {
    /// Provider identifier
    pub provider_id: ProviderId,
    /// Provider type
    pub provider_type: ProviderType,
    /// Health score (0.0 - 1.0)
    pub health_score: f64,
    /// Cost per hour
    pub cost_per_hour: f64,
    /// Estimated startup time
    pub startup_time: Duration,
    /// Available capacity (0.0 - 1.0)
    pub available_capacity: f64,
    /// Whether provider can accept workers
    pub can_accept_workers: bool,
}

impl ProviderScoreInput {
    /// Create from ProviderInfo
    pub fn from_provider_info(
        provider_id: ProviderId,
        provider_type: ProviderType,
        health_score: f64,
        cost_per_hour: f64,
        startup_time: Duration,
        active_workers: usize,
        max_workers: usize,
    ) -> Self {
        let available_capacity = if max_workers == 0 {
            0.0
        } else {
            1.0 - (active_workers as f64 / max_workers as f64)
        };

        Self {
            provider_id,
            provider_type,
            health_score,
            cost_per_hour,
            startup_time,
            available_capacity,
            can_accept_workers: active_workers < max_workers,
        }
    }

    /// Get available capacity
    pub fn available_capacity(&self) -> f64 {
        self.available_capacity
    }
}

/// Weights for combining score components
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringWeights {
    /// Weight for health score (default: 0.25)
    pub health: f64,
    /// Weight for cost efficiency (default: 0.25)
    pub cost: f64,
    /// Weight for startup time (default: 0.25)
    pub startup: f64,
    /// Weight for capacity (default: 0.25)
    pub capacity: f64,
    /// Custom weights
    pub custom: std::collections::HashMap<String, f64>,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            health: 0.25,
            cost: 0.25,
            startup: 0.25,
            capacity: 0.25,
            custom: std::collections::HashMap::new(),
        }
    }
}

impl ScoringWeights {
    /// Create weights with all components equal
    pub fn balanced() -> Self {
        Self::default()
    }

    /// Create weights optimized for cost
    pub fn cost_optimized() -> Self {
        Self {
            health: 0.15,
            cost: 0.50,
            startup: 0.20,
            capacity: 0.15,
            custom: std::collections::HashMap::new(),
        }
    }

    /// Create weights optimized for performance
    pub fn performance_optimized() -> Self {
        Self {
            health: 0.20,
            cost: 0.10,
            startup: 0.50,
            capacity: 0.20,
            custom: std::collections::HashMap::new(),
        }
    }

    /// Create weights for health-critical workloads
    pub fn health_critical() -> Self {
        Self {
            health: 0.50,
            cost: 0.15,
            startup: 0.20,
            capacity: 0.15,
            custom: std::collections::HashMap::new(),
        }
    }

    /// Set a custom weight
    pub fn with_custom_weight(mut self, name: &str, weight: f64) -> Self {
        self.custom.insert(name.to_string(), weight);
        self
    }

    /// Validate weights sum to 1.0 (with tolerance)
    pub fn is_valid(&self) -> bool {
        let sum = self.health
            + self.cost
            + self.startup
            + self.capacity
            + self.custom.values().sum::<f64>();
        (sum - 1.0).abs() < 0.001
    }
}

/// Trait for scoring providers based on a single criterion
///
/// This trait enables strategy composition by allowing individual scoring
/// criteria to be implemented independently and then combined.
pub trait ProviderScoring: Send + Sync {
    /// Get the name of this scoring criterion
    fn name(&self) -> &'static str;

    /// Score a provider for a job
    ///
    /// Returns a score between 0.0 and 1.0, where higher is better.
    fn score(&self, job: &Job, provider: &ProviderScoreInput) -> f64;

    /// Calculate health penalty multiplier
    ///
    /// Healthier providers get lower multipliers (better scores).
    fn health_penalty(&self, health_score: f64) -> f64 {
        match health_score {
            score if score >= 0.9 => 1.0, // Excellent - no penalty
            score if score >= 0.7 => 1.2, // Good - 20% penalty
            score if score >= 0.5 => 1.5, // Degraded - 50% penalty
            _ => 2.0,                     // Unhealthy - 100% penalty
        }
    }
}

// =============================================================================
// Standard Scoring Implementations
// =============================================================================

/// Scoring based on cost efficiency
#[derive(Debug, Clone, Default)]
pub struct CostScoring;

impl ProviderScoring for CostScoring {
    fn name(&self) -> &'static str {
        "cost"
    }

    fn score(&self, _job: &Job, provider: &ProviderScoreInput) -> f64 {
        if !provider.can_accept_workers {
            return 0.0;
        }

        // Calculate effective cost with health and capacity adjustments
        let health_multiplier = self.health_penalty(provider.health_score);
        let capacity_bonus = 1.0 - (provider.available_capacity() * 0.1); // Up to 10% discount

        let effective_cost = provider.cost_per_hour * health_multiplier * capacity_bonus;

        // Normalize: lower cost is better, so we return (1 / (1 + effective_cost))
        // For free providers, return 1.0
        if effective_cost <= 0.0 {
            return 1.0;
        }

        1.0 / (1.0 + effective_cost)
    }
}

/// Scoring based on startup time
#[derive(Debug, Clone, Default)]
pub struct StartupTimeScoring;

impl ProviderScoring for StartupTimeScoring {
    fn name(&self) -> &'static str {
        "startup_time"
    }

    fn score(&self, _job: &Job, provider: &ProviderScoreInput) -> f64 {
        if !provider.can_accept_workers {
            return 0.0;
        }

        let health_multiplier = self.health_penalty(provider.health_score);
        let capacity_penalty = 1.0 + (provider.available_capacity * 0.1);

        let effective_startup_ms =
            provider.startup_time.as_millis() as f64 * health_multiplier * capacity_penalty;

        // Normalize: lower startup is better
        // Return 1.0 for instant providers, lower for slower ones
        if effective_startup_ms <= 0.0 {
            return 1.0;
        }

        1.0 / (1.0 + effective_startup_ms / 1000.0) // Normalize to seconds
    }
}

/// Scoring based on health status
#[derive(Debug, Clone, Default)]
pub struct HealthScoring;

impl ProviderScoring for HealthScoring {
    fn name(&self) -> &'static str {
        "health"
    }

    fn score(&self, _job: &Job, provider: &ProviderScoreInput) -> f64 {
        // Health is already normalized 0-1, just ensure provider can accept workers
        if !provider.can_accept_workers {
            return 0.0;
        }

        // Only consider providers with acceptable health (>0.5)
        if provider.health_score < 0.5 {
            return 0.0;
        }

        provider.health_score
    }
}

/// Scoring based on available capacity
#[derive(Debug, Clone, Default)]
pub struct CapacityScoring;

impl ProviderScoring for CapacityScoring {
    fn name(&self) -> &'static str {
        "capacity"
    }

    fn score(&self, _job: &Job, provider: &ProviderScoreInput) -> f64 {
        if !provider.can_accept_workers {
            return 0.0;
        }

        provider.available_capacity
    }
}

/// Composite scoring that combines multiple criteria
///
/// Combines multiple ProviderScoring implementations with configurable weights.
pub struct CompositeProviderScoring {
    /// Scoring criteria to use
    criteria: Vec<Box<dyn ProviderScoring>>,
    /// Weights for combining scores
    weights: ScoringWeights,
}

impl CompositeProviderScoring {
    /// Create a new composite scorer
    pub fn new(weights: ScoringWeights) -> Self {
        Self {
            criteria: Vec::new(),
            weights,
        }
    }

    /// Add a scoring criterion
    pub fn with_criterion(mut self, criterion: impl ProviderScoring + 'static) -> Self {
        self.criteria.push(Box::new(criterion));
        self
    }

    /// Add cost scoring
    pub fn with_cost_scoring(self) -> Self {
        self.with_criterion(CostScoring)
    }

    /// Add startup time scoring
    pub fn with_startup_scoring(self) -> Self {
        self.with_criterion(StartupTimeScoring)
    }

    /// Add health scoring
    pub fn with_health_scoring(self) -> Self {
        self.with_criterion(HealthScoring)
    }

    /// Add capacity scoring
    pub fn with_capacity_scoring(self) -> Self {
        self.with_criterion(CapacityScoring)
    }

    /// Add all standard criteria with balanced weights
    pub fn with_balanced_scoring(self) -> Self {
        self.with_cost_scoring()
            .with_startup_scoring()
            .with_health_scoring()
            .with_capacity_scoring()
    }

    /// Build the composite scorer
    pub fn build(self) -> Self {
        self
    }
}

impl ProviderScoring for CompositeProviderScoring {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn score(&self, job: &Job, provider: &ProviderScoreInput) -> f64 {
        if self.criteria.is_empty() {
            return 0.0;
        }

        let total_weight: f64 = self
            .criteria
            .iter()
            .map(|c| match c.name() {
                "cost" => self.weights.cost,
                "startup_time" => self.weights.startup,
                "health" => self.weights.health,
                "capacity" => self.weights.capacity,
                _ => self.weights.custom.get(c.name()).copied().unwrap_or(0.0),
            })
            .sum();

        if total_weight == 0.0 {
            return 0.0;
        }

        let mut weighted_sum = 0.0;

        for criterion in &self.criteria {
            let raw_score = criterion.score(job, provider);
            let weight = match criterion.name() {
                "cost" => self.weights.cost,
                "startup_time" => self.weights.startup,
                "health" => self.weights.health,
                "capacity" => self.weights.capacity,
                _ => self
                    .weights
                    .custom
                    .get(criterion.name())
                    .copied()
                    .unwrap_or(0.0),
            };

            weighted_sum += raw_score * weight;
        }

        weighted_sum / total_weight
    }
}

/// Composite provider selector using scoring
///
/// A provider selector that uses composite scoring to select the best provider.
pub struct CompositeProviderSelector {
    /// Scoring strategy to use
    scoring: CompositeProviderScoring,
    /// Prefer higher scores (true) or lower (false)
    prefer_higher: bool,
}

impl CompositeProviderSelector {
    /// Create a new selector with balanced scoring
    pub fn balanced() -> Self {
        Self {
            scoring: CompositeProviderScoring::new(ScoringWeights::balanced())
                .with_balanced_scoring(),
            prefer_higher: true,
        }
    }

    /// Create a selector optimized for cost
    pub fn cost_optimized() -> Self {
        Self {
            scoring: CompositeProviderScoring::new(ScoringWeights::cost_optimized())
                .with_cost_scoring()
                .with_health_scoring()
                .with_capacity_scoring(),
            prefer_higher: true,
        }
    }

    /// Create a selector optimized for performance
    pub fn performance_optimized() -> Self {
        Self {
            scoring: CompositeProviderScoring::new(ScoringWeights::performance_optimized())
                .with_startup_scoring()
                .with_health_scoring()
                .with_capacity_scoring(),
            prefer_higher: true,
        }
    }

    /// Create a selector optimized for health
    pub fn health_optimized() -> Self {
        Self {
            scoring: CompositeProviderScoring::new(ScoringWeights::health_critical())
                .with_health_scoring()
                .with_cost_scoring()
                .with_startup_scoring(),
            prefer_higher: true,
        }
    }

    /// Create a custom selector
    pub fn custom(scoring: CompositeProviderScoring) -> Self {
        Self {
            scoring,
            prefer_higher: true,
        }
    }

    /// Set whether to prefer higher scores
    pub fn prefer_higher(mut self, prefer_higher: bool) -> Self {
        self.prefer_higher = prefer_higher;
        self
    }

    /// Select the best provider from a list
    pub fn select_provider(
        &self,
        job: &Job,
        providers: &[crate::scheduling::ProviderInfo],
    ) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        // Convert to score inputs
        let inputs: Vec<ProviderScoreInput> = providers
            .iter()
            .map(|p| {
                ProviderScoreInput::from_provider_info(
                    p.provider_id.clone(),
                    p.provider_type.clone(),
                    p.health_score,
                    p.cost_per_hour,
                    p.estimated_startup_time.clone(),
                    p.active_workers,
                    p.max_workers,
                )
            })
            .collect();

        // Score all providers
        let mut scored: Vec<(ProviderId, f64)> = providers
            .iter()
            .zip(inputs.iter())
            .map(|(provider, input)| {
                let score = self.scoring.score(job, input);
                (provider.provider_id.clone(), score)
            })
            .collect();

        // Sort by score
        scored.sort_by(|a, b| {
            if self.prefer_higher {
                b.1.partial_cmp(&a.1).unwrap()
            } else {
                a.1.partial_cmp(&b.1).unwrap()
            }
        });

        // Return the best provider
        scored.first().map(|(id, _)| id.clone())
    }
}

// =============================================================================
// Tests
// =============================================================================

// =============================================================================
// Scoring Results Cache
// =============================================================================

use crate::scheduling::ttl_cache::{LruTtlCache, SharedLruTtlCache};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Default TTL for scoring cache entries (30 seconds)
pub const SCORING_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);
/// Default maximum entries in scoring cache
pub const SCORING_CACHE_MAX_ENTRIES: usize = 1000;

/// Cache key for scoring results
///
/// This struct uniquely identifies a scoring request to enable caching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScoringCacheKey {
    /// Job identifier for cache grouping
    job_id: Arc<str>,
    /// Sorted provider IDs that were scored
    provider_ids: Arc<[Arc<str>]>,
    /// Hash of the weights configuration
    weights_hash: u64,
}

impl ScoringCacheKey {
    /// Create a new cache key from job and provider inputs
    pub fn new(job: &Job, provider_ids: &[ProviderId], weights: &ScoringWeights) -> Self {
        let job_id = Arc::from(job.id.to_string());

        let provider_ids_sorted: Vec<_> = provider_ids
            .iter()
            .map(|id| Arc::from(id.to_string()))
            .collect();
        let provider_ids = Arc::from(provider_ids_sorted.into_boxed_slice());

        // Generate a hash from weights
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        weights.health.to_bits().hash(&mut hasher);
        weights.cost.to_bits().hash(&mut hasher);
        weights.startup.to_bits().hash(&mut hasher);
        weights.capacity.to_bits().hash(&mut hasher);
        for (k, v) in &weights.custom {
            k.hash(&mut hasher);
            v.to_bits().hash(&mut hasher);
        }
        let weights_hash = hasher.finish();

        Self {
            job_id,
            provider_ids,
            weights_hash,
        }
    }
}

/// Cached scoring result
#[derive(Debug, Clone)]
pub struct ScoredResult {
    /// The selected provider ID
    pub selected_provider: ProviderId,
    /// All provider scores
    pub provider_scores: Vec<(ProviderId, f64)>,
    /// When this result was computed
    pub computed_at: std::time::Instant,
}

/// Shared scoring cache type
pub type SharedScoringCache = Arc<ScoringCache>;

/// Thread-safe scoring cache for provider selection results
///
/// This cache stores the results of provider scoring operations to avoid
/// recomputing scores for identical inputs within the TTL window.
pub struct ScoringCache {
    /// The underlying LRU TTL cache
    cache: SharedLruTtlCache<ScoringCacheKey, ScoredResult>,
}

impl ScoringCache {
    /// Create a new scoring cache with default settings
    pub fn new() -> Self {
        Self {
            cache: LruTtlCache::new(SCORING_CACHE_TTL, SCORING_CACHE_MAX_ENTRIES).into(),
        }
    }

    /// Get a shared reference to the cache
    pub fn shared() -> SharedScoringCache {
        Arc::new(Self::new())
    }

    /// Get a cached scoring result
    pub fn get(&self, key: &ScoringCacheKey) -> Option<ScoredResult> {
        self.cache.get(key)
    }

    /// Store a scoring result in the cache
    pub fn insert(&self, key: ScoringCacheKey, result: ScoredResult) {
        self.cache.insert(key, result);
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            // Note: Hit/miss tracking would require additional instrumentation
        }
    }
}

/// Statistics about the scoring cache
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in the cache
    pub entries: usize,
}

impl Default for ScoringCache {
    fn default() -> Self {
        Self::new()
    }
}

/// A provider selector with built-in caching
///
/// Wraps a `CompositeProviderSelector` with an LRU TTL cache to avoid
/// recomputing provider scores for identical inputs.
pub struct CachedProviderSelector {
    /// The underlying selector
    selector: CompositeProviderSelector,
    /// Cache for scoring results
    cache: SharedScoringCache,
    /// TTL for cache entries
    ttl: std::time::Duration,
}

impl CachedProviderSelector {
    /// Create a new cached selector with default TTL
    pub fn new() -> Self {
        Self::with_cache_and_ttl(
            CompositeProviderSelector::balanced(),
            ScoringCache::shared(),
            SCORING_CACHE_TTL,
        )
    }

    /// Create a cached selector with balanced scoring
    pub fn balanced() -> Self {
        Self::with_cache_and_ttl(
            CompositeProviderSelector::balanced(),
            ScoringCache::shared(),
            SCORING_CACHE_TTL,
        )
    }

    /// Create a cached selector optimized for cost
    pub fn cost_optimized() -> Self {
        Self::with_cache_and_ttl(
            CompositeProviderSelector::cost_optimized(),
            ScoringCache::shared(),
            SCORING_CACHE_TTL,
        )
    }

    /// Create a cached selector optimized for performance
    pub fn performance_optimized() -> Self {
        Self::with_cache_and_ttl(
            CompositeProviderSelector::performance_optimized(),
            ScoringCache::shared(),
            SCORING_CACHE_TTL,
        )
    }

    /// Create a cached selector optimized for health
    pub fn health_optimized() -> Self {
        Self::with_cache_and_ttl(
            CompositeProviderSelector::health_optimized(),
            ScoringCache::shared(),
            SCORING_CACHE_TTL,
        )
    }

    /// Create a cached selector with custom settings
    pub fn with_cache_and_ttl(
        selector: CompositeProviderSelector,
        cache: SharedScoringCache,
        ttl: std::time::Duration,
    ) -> Self {
        Self {
            selector,
            cache,
            ttl,
        }
    }

    /// Select the best provider from a list, using the cache
    ///
    /// If the same job and provider set has been scored before with the same
    /// weights, the cached result is returned.
    pub fn select_provider_cached(
        &self,
        job: &Job,
        providers: &[crate::scheduling::ProviderInfo],
    ) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        let provider_ids: Vec<ProviderId> =
            providers.iter().map(|p| p.provider_id.clone()).collect();

        // Build cache key
        let cache_key = ScoringCacheKey::new(job, &provider_ids, &self.selector.scoring.weights);

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key) {
            return Some(cached.selected_provider);
        }

        // Compute scores using the underlying selector
        let selected = self.selector.select_provider(job, providers)?;

        // Convert providers to score inputs for full scoring
        let inputs: Vec<ProviderScoreInput> = providers
            .iter()
            .map(|p| {
                ProviderScoreInput::from_provider_info(
                    p.provider_id.clone(),
                    p.provider_type.clone(),
                    p.health_score,
                    p.cost_per_hour,
                    p.estimated_startup_time.clone(),
                    p.active_workers,
                    p.max_workers,
                )
            })
            .collect();

        // Compute all scores for the cache
        let provider_scores: Vec<(ProviderId, f64)> = providers
            .iter()
            .zip(inputs.iter())
            .map(|(provider, input)| {
                let score = self.selector.scoring.score(job, input);
                (provider.provider_id.clone(), score)
            })
            .collect();

        // Store in cache
        let result = ScoredResult {
            selected_provider: selected.clone(),
            provider_scores,
            computed_at: std::time::Instant::now(),
        };
        self.cache.insert(cache_key, result);

        Some(selected)
    }

    /// Get the underlying selector for direct access
    pub fn inner(&self) -> &CompositeProviderSelector {
        &self.selector
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }
}

impl Default for CachedProviderSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod scoring_cache_tests {
    use super::*;
    use crate::jobs::Job;
    use crate::shared_kernel::{JobId, ProviderId};
    use crate::workers::ProviderType;
    use std::time::Duration;

    fn create_test_job() -> Job {
        Job::new(
            JobId::new(),
            crate::jobs::JobSpec::new(vec!["echo".to_string()]),
        )
    }

    fn create_test_provider_info(
        id: ProviderId,
        health_score: f64,
        cost: f64,
        startup_ms: u64,
        active: usize,
        max: usize,
    ) -> crate::scheduling::ProviderInfo {
        crate::scheduling::ProviderInfo {
            provider_id: id,
            provider_type: ProviderType::Docker,
            active_workers: active,
            max_workers: max,
            estimated_startup_time: Duration::from_millis(startup_ms),
            health_score,
            cost_per_hour: cost,
            gpu_support: false,
            gpu_types: vec![],
            regions: vec![],
        }
    }

    #[test]
    fn test_cost_scoring() {
        let scoring = CostScoring;
        let job = create_test_job();

        let low_cost = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 0.1,
            startup_time: Duration::from_secs(5),
            available_capacity: 0.5,
            can_accept_workers: true,
        };
        let score_low = scoring.score(&job, &low_cost);

        let high_cost = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 1.0,
            startup_time: Duration::from_secs(5),
            available_capacity: 0.5,
            can_accept_workers: true,
        };
        let score_high = scoring.score(&job, &high_cost);

        assert!(score_low > score_high);
    }

    #[test]
    fn test_startup_time_scoring() {
        let scoring = StartupTimeScoring;
        let job = create_test_job();

        let fast = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 0.1,
            startup_time: Duration::from_millis(1000),
            available_capacity: 0.5,
            can_accept_workers: true,
        };
        let score_fast = scoring.score(&job, &fast);

        let slow = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 0.1,
            startup_time: Duration::from_millis(30000),
            available_capacity: 0.5,
            can_accept_workers: true,
        };
        let score_slow = scoring.score(&job, &slow);

        assert!(score_fast > score_slow);
    }

    #[test]
    fn test_health_scoring() {
        let scoring = HealthScoring;
        let job = create_test_job();

        let healthy = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 0.1,
            startup_time: Duration::from_secs(5),
            available_capacity: 0.5,
            can_accept_workers: true,
        };
        let score_healthy = scoring.score(&job, &healthy);

        let unhealthy = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.3,
            cost_per_hour: 0.1,
            startup_time: Duration::from_secs(5),
            available_capacity: 0.5,
            can_accept_workers: true,
        };
        let score_unhealthy = scoring.score(&job, &unhealthy);

        assert_eq!(score_healthy, 0.95);
        assert_eq!(score_unhealthy, 0.0); // Below threshold
    }

    #[test]
    fn test_capacity_scoring() {
        let scoring = CapacityScoring;
        let job = create_test_job();

        let high_capacity = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 0.1,
            startup_time: Duration::from_secs(5),
            available_capacity: 0.9,
            can_accept_workers: true,
        };
        let score_high = scoring.score(&job, &high_capacity);

        let low_capacity = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 0.1,
            startup_time: Duration::from_secs(5),
            available_capacity: 0.1,
            can_accept_workers: true,
        };
        let score_low = scoring.score(&job, &low_capacity);

        assert!(score_high > score_low);
    }

    #[test]
    fn test_composite_scoring() {
        let composite = CompositeProviderScoring::new(ScoringWeights::balanced())
            .with_balanced_scoring()
            .build();

        let job = create_test_job();

        let provider1 = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            health_score: 0.95,
            cost_per_hour: 0.1,
            startup_time: Duration::from_millis(1000),
            available_capacity: 0.9,
            can_accept_workers: true,
        };
        let provider2 = ProviderScoreInput {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Kubernetes,
            health_score: 0.5,
            cost_per_hour: 1.0,
            startup_time: Duration::from_millis(30000),
            available_capacity: 0.1,
            can_accept_workers: true,
        };

        let score1 = composite.score(&job, &provider1);
        let score2 = composite.score(&job, &provider2);

        assert!(score1 > score2);
    }

    #[test]
    fn test_scoring_weights_validation() {
        let weights = ScoringWeights::default();
        assert!(weights.is_valid());

        let cost_weights = ScoringWeights::cost_optimized();
        assert!(cost_weights.is_valid());
    }

    #[test]
    fn test_composite_provider_selector() {
        let selector = CompositeProviderSelector::balanced();

        let providers = vec![
            create_test_provider_info(ProviderId::new(), 0.95, 0.1, 5000, 2, 10),
            create_test_provider_info(ProviderId::new(), 0.7, 0.5, 30000, 5, 20),
        ];

        let job = create_test_job();
        let selected = selector.select_provider(&job, &providers);

        assert!(selected.is_some());
        // Should select provider 1 (Docker) due to better overall score
        assert_eq!(selected.unwrap(), providers[0].provider_id);
    }

    #[test]
    fn test_provider_score_input_from_provider_info() {
        let info = create_test_provider_info(ProviderId::new(), 0.9, 0.5, 5000, 5, 10);
        let input = ProviderScoreInput::from_provider_info(
            info.provider_id.clone(),
            info.provider_type.clone(),
            info.health_score,
            info.cost_per_hour,
            info.estimated_startup_time.clone(),
            info.active_workers,
            info.max_workers,
        );

        assert_eq!(input.provider_id, info.provider_id);
        assert_eq!(input.provider_type, info.provider_type);
        assert!((input.available_capacity() - 0.5).abs() < 0.01);
    }

    // Scoring cache tests
    #[test]
    fn test_scoring_cache_insert_and_get() {
        let cache = ScoringCache::new();
        let job = create_test_job();
        let provider_ids = vec![ProviderId::new(), ProviderId::new()];
        let weights = ScoringWeights::balanced();

        let cache_key = ScoringCacheKey::new(&job, &provider_ids, &weights);
        let result = ScoredResult {
            selected_provider: provider_ids[0].clone(),
            provider_scores: vec![
                (provider_ids[0].clone(), 0.9),
                (provider_ids[1].clone(), 0.5),
            ],
            computed_at: std::time::Instant::now(),
        };

        cache.insert(cache_key.clone(), result);

        assert!(cache.get(&cache_key).is_some());
    }

    #[test]
    fn test_scoring_cache_miss() {
        let cache = ScoringCache::new();
        let job = create_test_job();
        let provider_ids = vec![ProviderId::new()];
        let weights = ScoringWeights::balanced();

        let cache_key = ScoringCacheKey::new(&job, &provider_ids, &weights);

        assert!(cache.get(&cache_key).is_none());
    }

    #[test]
    fn test_cached_provider_selector_selects() {
        let selector = CachedProviderSelector::balanced();

        let providers = vec![
            create_test_provider_info(ProviderId::new(), 0.95, 0.1, 5000, 2, 10),
            create_test_provider_info(ProviderId::new(), 0.7, 0.5, 30000, 5, 20),
        ];

        let job = create_test_job();
        let selected = selector.select_provider_cached(&job, &providers);

        assert!(selected.is_some());
    }

    #[test]
    fn test_cached_provider_selector_caches_result() {
        let cache: SharedScoringCache = ScoringCache::shared();
        let selector = CachedProviderSelector::with_cache_and_ttl(
            CompositeProviderSelector::balanced(),
            cache.clone(),
            SCORING_CACHE_TTL,
        );

        let providers = vec![create_test_provider_info(
            ProviderId::new(),
            0.95,
            0.1,
            5000,
            2,
            10,
        )];

        let job = create_test_job();

        // First call - should compute and cache
        let first = selector.select_provider_cached(&job, &providers);
        assert!(first.is_some());
        assert_eq!(cache.stats().entries, 1);

        // Second call - should return cached result
        let second = selector.select_provider_cached(&job, &providers);
        assert_eq!(first, second);
        // Cache entry should remain the same (hit, not new entry)
        assert_eq!(cache.stats().entries, 1);
    }

    #[test]
    fn test_scoring_cache_stats() {
        let cache = ScoringCache::new();
        let job = create_test_job();
        let weights = ScoringWeights::balanced();

        assert_eq!(cache.stats().entries, 0);

        // Add some entries
        for _ in 0..5 {
            let provider_ids = vec![ProviderId::new()];
            let cache_key = ScoringCacheKey::new(&job, &provider_ids, &weights);
            let result = ScoredResult {
                selected_provider: provider_ids[0].clone(),
                provider_scores: vec![],
                computed_at: std::time::Instant::now(),
            };
            cache.insert(cache_key, result);
        }

        assert_eq!(cache.stats().entries, 5);
    }
}
