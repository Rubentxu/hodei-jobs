//! Performance Benchmarks for Scheduling Module
//!
//! Benchmarks for critical hot paths in the scheduling system.
//! Run with: cargo bench -p hodei-server-domain

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;

// Re-export modules for use in benchmarks
use hodei_server_domain::jobs::{Job, JobSpec};
use hodei_server_domain::scheduling::{
    self, CachedProviderSelector, CapacityScoring, CompositeProviderScoring, CostScoring,
    FastestStartupProviderSelector, HealthiestProviderSelector, LowestCostProviderSelector,
    MostCapacityProviderSelector, ProviderScoreInput, ProviderScoring, ProviderSelector,
    ProviderTypeMapping, ScoringWeights, StartupTimeScoring,
};
use hodei_server_domain::shared_kernel::JobId;
use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::ProviderType;

/// Create test providers for benchmarks
fn create_test_providers() -> Vec<scheduling::ProviderInfo> {
    vec![
        scheduling::ProviderInfo {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            active_workers: 5,
            max_workers: 10,
            estimated_startup_time: Duration::from_millis(500),
            health_score: 0.95,
            cost_per_hour: 0.1,
            gpu_support: false,
            gpu_types: vec![],
            regions: vec!["local".to_string()],
        },
        scheduling::ProviderInfo {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Kubernetes,
            active_workers: 10,
            max_workers: 50,
            estimated_startup_time: Duration::from_millis(30000),
            health_score: 0.88,
            cost_per_hour: 0.5,
            gpu_support: true,
            gpu_types: vec!["nvidia-tesla-v100".to_string()],
            regions: vec!["us-east-1".to_string(), "eu-west-1".to_string()],
        },
        scheduling::ProviderInfo {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Lambda,
            active_workers: 20,
            max_workers: 100,
            estimated_startup_time: Duration::from_millis(2000),
            health_score: 0.92,
            cost_per_hour: 0.05,
            gpu_support: false,
            gpu_types: vec![],
            regions: vec!["us-east-1".to_string()],
        },
        scheduling::ProviderInfo {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Fargate,
            active_workers: 8,
            max_workers: 30,
            estimated_startup_time: Duration::from_millis(15000),
            health_score: 0.85,
            cost_per_hour: 0.3,
            gpu_support: false,
            gpu_types: vec![],
            regions: vec!["us-west-2".to_string()],
        },
    ]
}

fn create_test_job() -> Job {
    Job::new(
        JobId::new(),
        "benchmark-test-job".to_string(),
        JobSpec::new(vec!["echo".to_string(), "hello".to_string()]),
    )
}

// =============================================================================
// Provider Preference Benchmarks
// =============================================================================

fn bench_provider_preference_new(c: &mut Criterion) {
    c.bench_function("provider_preference_new", |b| {
        b.iter(|| {
            let _ = black_box(scheduling::ProviderPreference::new("kubernetes").unwrap());
        });
    });
}

fn bench_provider_preference_matches(c: &mut Criterion) {
    let pref = scheduling::ProviderPreference::new("k8s").unwrap();
    let provider_type = ProviderType::Kubernetes;

    c.bench_function("provider_preference_matches", |b| {
        b.iter(|| black_box(pref.matches_provider_type(&provider_type)));
    });
}

fn bench_provider_preference_matches_alias(c: &mut Criterion) {
    let pref = scheduling::ProviderPreference::new("kube").unwrap();
    let provider_type = ProviderType::Kubernetes;

    c.bench_function("provider_preference_matches_alias", |b| {
        b.iter(|| black_box(pref.matches_provider_type(&provider_type)));
    });
}

// =============================================================================
// ProviderTypeMapping Benchmarks
// =============================================================================

fn bench_provider_type_mapping_match(c: &mut Criterion) {
    let mapping = ProviderTypeMapping::new();

    c.bench_function("provider_type_mapping_match", |b| {
        b.iter(|| black_box(mapping.match_provider("kubernetes")));
    });
}

fn bench_provider_type_mapping_match_alias(c: &mut Criterion) {
    let mapping = ProviderTypeMapping::new();

    c.bench_function("provider_type_mapping_match_alias", |b| {
        b.iter(|| black_box(mapping.match_provider("k8s")));
    });
}

fn bench_provider_type_mapping_cached(c: &mut Criterion) {
    let mapping = ProviderTypeMapping::new();
    // Warm up the cache
    let _ = mapping.match_provider("kubernetes");

    c.bench_function("provider_type_mapping_cached", |b| {
        b.iter(|| black_box(mapping.match_provider("kubernetes")));
    });
}

fn bench_provider_type_mapping_is_provider_type(c: &mut Criterion) {
    let mapping = ProviderTypeMapping::new();

    c.bench_function("provider_type_mapping_is_provider_type", |b| {
        b.iter(|| black_box(mapping.is_provider_type("k8s", &ProviderType::Kubernetes)));
    });
}

// =============================================================================
// Provider Scoring Benchmarks
// =============================================================================

fn bench_cost_scoring(c: &mut Criterion) {
    let scoring = CostScoring;
    let job = create_test_job();
    let provider = ProviderScoreInput {
        provider_id: ProviderId::new(),
        provider_type: ProviderType::Docker,
        health_score: 0.95,
        cost_per_hour: 0.1,
        startup_time: Duration::from_millis(500),
        available_capacity: 0.5,
        can_accept_workers: true,
    };

    c.bench_function("cost_scoring", |b| {
        b.iter(|| black_box(scoring.score(&job, &provider)));
    });
}

fn bench_startup_time_scoring(c: &mut Criterion) {
    let scoring = StartupTimeScoring;
    let job = create_test_job();
    let provider = ProviderScoreInput {
        provider_id: ProviderId::new(),
        provider_type: ProviderType::Docker,
        health_score: 0.95,
        cost_per_hour: 0.1,
        startup_time: Duration::from_millis(500),
        available_capacity: 0.5,
        can_accept_workers: true,
    };

    c.bench_function("startup_time_scoring", |b| {
        b.iter(|| black_box(scoring.score(&job, &provider)));
    });
}

fn bench_capacity_scoring(c: &mut Criterion) {
    let scoring = CapacityScoring;
    let job = create_test_job();
    let provider = ProviderScoreInput {
        provider_id: ProviderId::new(),
        provider_type: ProviderType::Docker,
        health_score: 0.95,
        cost_per_hour: 0.1,
        startup_time: Duration::from_millis(500),
        available_capacity: 0.5,
        can_accept_workers: true,
    };

    c.bench_function("capacity_scoring", |b| {
        b.iter(|| black_box(scoring.score(&job, &provider)));
    });
}

fn bench_composite_scoring(c: &mut Criterion) {
    let composite = CompositeProviderScoring::new(ScoringWeights::balanced())
        .with_balanced_scoring()
        .build();
    let job = create_test_job();
    let provider = ProviderScoreInput {
        provider_id: ProviderId::new(),
        provider_type: ProviderType::Docker,
        health_score: 0.95,
        cost_per_hour: 0.1,
        startup_time: Duration::from_millis(500),
        available_capacity: 0.5,
        can_accept_workers: true,
    };

    c.bench_function("composite_scoring", |b| {
        b.iter(|| black_box(composite.score(&job, &provider)));
    });
}

// =============================================================================
// Provider Selection Benchmarks
// =============================================================================

fn bench_lowest_cost_selection(c: &mut Criterion) {
    let selector = LowestCostProviderSelector::new();
    let providers = create_test_providers();
    let job = create_test_job();

    c.bench_function("lowest_cost_selection", |b| {
        b.iter(|| black_box(selector.select_provider(&job, &providers)));
    });
}

fn bench_fastest_startup_selection(c: &mut Criterion) {
    let selector = FastestStartupProviderSelector::new();
    let providers = create_test_providers();
    let job = create_test_job();

    c.bench_function("fastest_startup_selection", |b| {
        b.iter(|| black_box(selector.select_provider(&job, &providers)));
    });
}

fn bench_most_capacity_selection(c: &mut Criterion) {
    let selector = MostCapacityProviderSelector::new();
    let providers = create_test_providers();
    let job = create_test_job();

    c.bench_function("most_capacity_selection", |b| {
        b.iter(|| black_box(selector.select_provider(&job, &providers)));
    });
}

fn bench_healthiest_selection(c: &mut Criterion) {
    let selector = HealthiestProviderSelector::new();
    let providers = create_test_providers();
    let job = create_test_job();

    c.bench_function("healthiest_selection", |b| {
        b.iter(|| black_box(selector.select_provider(&job, &providers)));
    });
}

fn bench_cached_selector(c: &mut Criterion) {
    let selector = CachedProviderSelector::balanced();
    let providers = create_test_providers();
    let job = create_test_job();

    // Warm up
    let _ = selector.select_provider_cached(&job, &providers);

    c.bench_function("cached_selector", |b| {
        b.iter(|| black_box(selector.select_provider_cached(&job, &providers)));
    });
}

// =============================================================================
// TTL Cache Benchmarks
// =============================================================================

fn bench_ttl_cache_insert(c: &mut Criterion) {
    use hodei_server_domain::scheduling::ttl_cache::TtlCache;

    let cache = TtlCache::new(Duration::from_secs(60));

    c.bench_function("ttl_cache_insert", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.insert(i, i * 2));
            }
        });
    });
}

fn bench_ttl_cache_get(c: &mut Criterion) {
    use hodei_server_domain::scheduling::ttl_cache::TtlCache;

    let cache = TtlCache::new(Duration::from_secs(60));
    // Populate cache
    for i in 0..100 {
        cache.insert(i, i * 2);
    }

    c.bench_function("ttl_cache_get", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.get(&i));
            }
        });
    });
}

fn bench_lru_ttl_cache_insert(c: &mut Criterion) {
    use hodei_server_domain::scheduling::ttl_cache::LruTtlCache;

    let cache = LruTtlCache::new(Duration::from_secs(60), 100);

    c.bench_function("lru_ttl_cache_insert", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.insert(i, i * 2));
            }
        });
    });
}

fn bench_lru_ttl_cache_get(c: &mut Criterion) {
    use hodei_server_domain::scheduling::ttl_cache::LruTtlCache;

    let cache = LruTtlCache::new(Duration::from_secs(60), 100);
    // Populate cache
    for i in 0..100 {
        cache.insert(i, i * 2);
    }

    c.bench_function("lru_ttl_cache_get", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.get(&i));
            }
        });
    });
}

// =============================================================================
// Benchmark Groups
// =============================================================================

criterion_group!(
    benches,
    bench_provider_preference_new,
    bench_provider_preference_matches,
    bench_provider_preference_matches_alias,
    bench_provider_type_mapping_match,
    bench_provider_type_mapping_match_alias,
    bench_provider_type_mapping_cached,
    bench_provider_type_mapping_is_provider_type,
    bench_cost_scoring,
    bench_startup_time_scoring,
    bench_capacity_scoring,
    bench_composite_scoring,
    bench_lowest_cost_selection,
    bench_fastest_startup_selection,
    bench_most_capacity_selection,
    bench_healthiest_selection,
    bench_cached_selector,
    bench_ttl_cache_insert,
    bench_ttl_cache_get,
    bench_lru_ttl_cache_insert,
    bench_lru_ttl_cache_get,
);

criterion_main!(benches);
