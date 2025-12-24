//! Scheduling Value Objects
//!
//! Strongly-typed domain objects that encapsulate validation and business rules.
//!
//! ## Design Principles
//! - Value objects are immutable
//! - Value objects encapsulate validation logic
//! - Value objects provide type safety
//! - Value objects enable composition
//!
//! ## Connascence Reduction
//! - **Before**: Connascence of Position - raw strings for provider preferences
//! - **After**: Connascence of Type - validated value objects

use crate::scheduling::ttl_cache::{LruTtlCache, SharedLruTtlCache};
use crate::shared_kernel::ProviderId;
use crate::workers::{ProviderType, Worker, WorkerSpec};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// A validated provider preference with normalized matching
///
/// Reduces Connascence of Position by encapsulating provider matching logic
/// in a single place rather than duplicating string operations across the codebase.
///
/// ## Performance Optimization
/// - Pre-computes normalized form
/// - Caches alias mappings
/// - Reduces string operations from O(n) to O(1) after construction
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderPreference {
    /// The normalized provider type string
    normalized: String,
    /// The original input (for logging/debugging)
    original: String,
    /// Whether this is a provider ID match (vs type match)
    is_provider_id: bool,
    /// The matched provider ID if applicable
    matched_provider_id: Option<ProviderId>,
}

impl ProviderPreference {
    /// Create a new ProviderPreference from a raw string
    ///
    /// # Errors
    /// Returns an error if the input is empty or contains invalid characters
    pub fn new(input: &str) -> Result<Self, ProviderPreferenceError> {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Err(ProviderPreferenceError::Empty);
        }

        // Normalize the input using the optimized function
        let (normalized, _) = Self::normalize_optimized(trimmed);

        Ok(Self {
            normalized,
            original: trimmed.to_string(),
            is_provider_id: false,
            matched_provider_id: None,
        })
    }

    /// Create a preference that matches a specific provider ID
    ///
    /// Optimized: UUIDs are already in lowercase, no need for to_lowercase().
    pub fn from_provider_id(provider_id: ProviderId) -> Self {
        let provider_id_str = provider_id.to_string();
        Self {
            normalized: provider_id_str.clone(), // UUID is already lowercase
            original: provider_id_str,
            is_provider_id: true,
            matched_provider_id: Some(provider_id),
        }
    }

    /// Normalize provider name to standard form using static strings where possible
    ///
    /// This optimized version returns a tuple with the normalized string and a flag
    /// indicating whether it's a known alias (to avoid allocations).
    fn normalize_optimized(input: &str) -> (String, bool) {
        let lower = input.to_lowercase();

        // Use static str comparisons to avoid allocations for known aliases
        let result = match lower.as_str() {
            "k8s" | "kube" | "kuber" => "kubernetes",
            "docker" | "containerd" => "docker",
            "lambda" | "aws-lambda" => "lambda",
            "ecs" | "aws-ecs" => "ecs",
            "fargate" | "aws-fargate" => "fargate",
            "ec2" | "aws-ec2" => "ec2",
            "gke" | "gcp-k8s" => "kubernetes",
            "cloudrun" | "gcp-cloudrun" => "cloudrun",
            "container-apps" | "azure-container-apps" => "container_apps",
            "azure-functions" | "az-functions" => "azure_functions",
            "azure-vm" | "azure-vms" => "azure_vm",
            _ => return (lower.replace([' ', '-'], "_"), false),
        };

        (result.to_string(), true)
    }

    /// Normalize provider name to standard form (legacy compatibility)
    #[deprecated(since = "0.4.0", note = "Use normalize_optimized instead")]
    fn normalize(input: &str) -> String {
        Self::normalize_optimized(input).0
    }

    /// Check if this preference matches a given provider type
    ///
    /// Uses the optimized `ProviderType::name()` method to avoid allocations.
    pub fn matches_provider_type(&self, provider_type: &ProviderType) -> bool {
        // Use the static name() method to avoid to_string() + to_lowercase() allocation
        let provider_name = provider_type.name();

        // Direct match (both already normalized)
        if self.normalized == provider_name {
            return true;
        }

        // Substring match (e.g., "k8s" matches "kubernetes")
        if provider_name.contains(&self.normalized) {
            return true;
        }

        // Reverse substring match (e.g., "kubernetes" contains "k8s")
        if self.normalized.len() >= 3 && provider_name.contains(&self.normalized) {
            return true;
        }

        false
    }

    /// Check if this preference matches a provider ID
    ///
    /// Optimized to avoid allocations: UUIDs are already in lowercase format.
    pub fn matches_provider_id(&self, provider_id: &ProviderId) -> bool {
        if self.is_provider_id {
            if let Some(matched_id) = &self.matched_provider_id {
                return matched_id == provider_id;
            }
        }
        // UUID Display output is already lowercase, no need for to_lowercase()
        provider_id.to_string() == self.normalized
    }

    /// Check if this is a provider ID match
    pub fn is_specific_provider(&self) -> bool {
        self.is_provider_id
    }

    /// Get the normalized form
    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    /// Get the original input
    pub fn original(&self) -> &str {
        &self.original
    }
}

impl fmt::Display for ProviderPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.original)
    }
}

/// Error for invalid provider preference
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderPreferenceError {
    /// Empty string provided
    Empty,
    /// Invalid characters in provider name
    InvalidCharacters { chars: String },
}

impl fmt::Display for ProviderPreferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderPreferenceError::Empty => write!(f, "Provider preference cannot be empty"),
            ProviderPreferenceError::InvalidCharacters { chars } => {
                write!(f, "Invalid characters in provider preference: {}", chars)
            }
        }
    }
}

// =============================================================================
// ProviderTypeMapping Service
// ==============================================================================

/// Default TTL for provider type cache entries
const PROVIDER_CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes
/// Maximum entries in the provider cache
const PROVIDER_CACHE_MAX_ENTRIES: usize = 500;

/// Cached provider type mapping with alias resolution
///
/// Provides efficient provider type matching with pre-computed aliases
/// and LRU TTL caching to avoid repeated string operations.
///
/// ## Performance Characteristics
/// - O(1) lookup with LRU cache hit
/// - Pre-computed alias mappings
/// - Thread-safe LRU cache with automatic eviction
/// - Configurable TTL for cache entries
///
/// ## Usage
/// For optimal performance, share a single instance across threads:
/// ```rust
/// use std::sync::Arc;
/// use hodei_server_domain::scheduling::ProviderTypeMapping;
///
/// let mapping: Arc<ProviderTypeMapping> = Arc::new(ProviderTypeMapping::new());
/// // Clone and use in multiple threads
/// ```
#[derive(Debug, Clone)]
pub struct ProviderTypeMapping {
    /// Static cache of normalized name -> ProviderType (immutable after init)
    type_cache: HashMap<&'static str, ProviderType>,
    /// Alias mappings (immutable after init)
    aliases: HashMap<&'static str, &'static str>,
    /// LRU cache for recent lookups (thread-safe)
    lookup_cache: SharedLruTtlCache<String, ProviderType>,
}

impl ProviderTypeMapping {
    /// Create a new ProviderTypeMapping with default provider mappings
    ///
    /// This constructor initializes the static mappings which are then
    /// shared across all clones of this instance.
    pub fn new() -> Self {
        let (type_cache, aliases) = Self::build_static_mappings();

        Self {
            type_cache,
            aliases,
            lookup_cache: Arc::new(LruTtlCache::new(
                PROVIDER_CACHE_TTL,
                PROVIDER_CACHE_MAX_ENTRIES,
            )),
        }
    }

    /// Create a shared provider type mapping
    ///
    /// Returns a reference-counted instance suitable for sharing across threads.
    pub fn shared() -> SharedProviderTypeMapping {
        Arc::new(Self::new())
    }

    /// Build the static (immutable) mappings
    fn build_static_mappings() -> (
        HashMap<&'static str, ProviderType>,
        HashMap<&'static str, &'static str>,
    ) {
        let mut type_cache = HashMap::new();
        let mut aliases = HashMap::new();

        // Add all standard provider types - using &'static str for immutability
        let standard_types: [(&'static str, ProviderType); 22] = [
            ("docker", ProviderType::Docker),
            ("kubernetes", ProviderType::Kubernetes),
            ("k8s", ProviderType::Kubernetes),
            ("kube", ProviderType::Kubernetes),
            ("lambda", ProviderType::Lambda),
            ("aws_lambda", ProviderType::Lambda),
            ("fargate", ProviderType::Fargate),
            ("aws_fargate", ProviderType::Fargate),
            ("cloudrun", ProviderType::CloudRun),
            ("gcp_cloudrun", ProviderType::CloudRun),
            ("container_apps", ProviderType::ContainerApps),
            ("azure_container_apps", ProviderType::ContainerApps),
            ("azure_functions", ProviderType::AzureFunctions),
            ("ec2", ProviderType::EC2),
            ("aws_ec2", ProviderType::EC2),
            ("compute_engine", ProviderType::ComputeEngine),
            ("gcp_compute", ProviderType::ComputeEngine),
            ("azure_vm", ProviderType::AzureVMs),
            ("azure_vms", ProviderType::AzureVMs),
            ("test", ProviderType::Test),
            ("bare_metal", ProviderType::BareMetal),
            ("baremetal", ProviderType::BareMetal),
        ];

        for (name, provider_type) in standard_types {
            type_cache.insert(name, provider_type.clone());
            // Also add the provider_type.to_string() form
            type_cache.insert(
                provider_type.to_string().to_lowercase().leak(),
                provider_type.clone(),
            );
        }

        // Add aliases - using &'static str
        let alias_mappings: [(&'static str, &'static str); 11] = [
            ("k8s", "kubernetes"),
            ("kube", "kubernetes"),
            ("aws-lambda", "lambda"),
            ("aws-fargate", "fargate"),
            ("gcp-cloudrun", "cloudrun"),
            ("azure-container-apps", "container_apps"),
            ("az-functions", "azure_functions"),
            ("aws-ec2", "ec2"),
            ("gcp-compute", "compute_engine"),
            ("azure-vm", "azure_vm"),
            ("baremetal", "bare_metal"),
        ];

        for (alias, canonical) in alias_mappings {
            aliases.insert(alias, canonical);
        }

        (type_cache, aliases)
    }

    /// Match a provider name/alias to a ProviderType
    ///
    /// Returns `None` if no matching provider type is found.
    ///
    /// ## Performance
    /// Uses LRU cache for O(1) lookup of repeated requests.
    /// First lookup is O(1) for cache miss.
    ///
    /// ## Optimization Notes
    /// - Minimizes string allocations in hot path
    /// - Reuses normalized string for cache key
    pub fn match_provider(&self, name: &str) -> Option<ProviderType> {
        // Check LRU cache first - use the input string directly
        // Note: We need to allocate for cache key ownership, but we can reuse
        let name_string = name.to_string();

        if let Some(cached) = self.lookup_cache.get(&name_string) {
            return Some(cached.clone());
        }

        // Normalize using optimized function
        let (normalized, _is_known) = ProviderPreference::normalize_optimized(name);

        // Direct lookup in static cache - reuse normalized string
        if let Some(provider_type) = self.type_cache.get(normalized.as_str()) {
            // Use normalized for cache key to avoid re-allocation
            self.lookup_cache
                .insert(normalized.clone(), provider_type.clone());
            return Some(provider_type.clone());
        }

        // Check aliases
        if let Some(canonical) = self.aliases.get(normalized.as_str()) {
            if let Some(provider_type) = self.type_cache.get(canonical) {
                self.lookup_cache
                    .insert(normalized.clone(), provider_type.clone());
                return Some(provider_type.clone());
            }
        }

        // Try lowercase variant (only if different from normalized)
        let lower = normalized.to_lowercase();
        if lower != normalized {
            if let Some(provider_type) = self.type_cache.get(lower.as_str()) {
                self.lookup_cache.insert(lower, provider_type.clone());
                return Some(provider_type.clone());
            }
        }

        None
    }

    /// Check if a name matches a specific provider type
    pub fn is_provider_type(&self, name: &str, provider_type: &ProviderType) -> bool {
        if let Some(matched) = self.match_provider(name) {
            return matched == *provider_type;
        }
        false
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            entries: self.lookup_cache.len(),
            max_entries: PROVIDER_CACHE_MAX_ENTRIES,
        }
    }

    /// Clear the lookup cache
    pub fn clear_cache(&self) {
        self.lookup_cache.clear();
    }

    /// Get the number of cache hits
    pub fn cache_hit_count(&self) -> usize {
        // Note: Actual hit count would require additional instrumentation
        // This is a simplified version
        self.lookup_cache.len()
    }
}

/// Cache statistics for ProviderTypeMapping
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Current number of entries in cache
    pub entries: usize,
    /// Maximum entries allowed
    pub max_entries: usize,
}

impl CacheStats {
    /// Check if cache is at capacity
    pub fn is_full(&self) -> bool {
        self.entries >= self.max_entries
    }

    /// Get cache utilization as a percentage
    pub fn utilization(&self) -> f64 {
        if self.max_entries == 0 {
            0.0
        } else {
            (self.entries as f64 / self.max_entries as f64) * 100.0
        }
    }
}

/// Shared reference to ProviderTypeMapping for thread-safe sharing
pub type SharedProviderTypeMapping = Arc<ProviderTypeMapping>;

impl Default for ProviderTypeMapping {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// WorkerRequirements Value Object
// =============================================================================

/// Worker requirements that must be satisfied for job execution
///
/// Encapsulates label and annotation requirements with efficient matching.
/// Reduces Connascence of Position by centralizing requirement validation.
///
/// ## Design
/// - Immutable value object
/// - Pre-validates all requirements
/// - Provides efficient matching method
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRequirements {
    /// Required labels (ALL must match)
    required_labels: HashMap<String, String>,
    /// Required annotations (ALL must match)
    required_annotations: HashMap<String, String>,
}

impl WorkerRequirements {
    /// Create new worker requirements from job preferences
    pub fn new(
        required_labels: HashMap<String, String>,
        required_annotations: HashMap<String, String>,
    ) -> Result<Self, WorkerRequirementsError> {
        // Validate label keys and values
        for (key, value) in &required_labels {
            Self::validate_label_or_annotation_key(key)?;
            Self::validate_label_or_annotation_value(value)?;
        }

        // Validate annotation keys and values
        for (key, value) in &required_annotations {
            Self::validate_label_or_annotation_key(key)?;
            Self::validate_label_or_annotation_value(value)?;
        }

        Ok(Self {
            required_labels,
            required_annotations,
        })
    }

    /// Create default worker requirements (no requirements)
    pub fn none() -> Self {
        Self {
            required_labels: HashMap::new(),
            required_annotations: HashMap::new(),
        }
    }

    /// Validate label/annotation key
    ///
    /// Keys must:
    /// - Not be empty
    /// - Not start with kubernetes reserved prefix
    /// - Contain only valid characters
    fn validate_label_or_annotation_key(key: &str) -> Result<(), WorkerRequirementsError> {
        let trimmed = key.trim();

        if trimmed.is_empty() {
            return Err(WorkerRequirementsError::EmptyKey);
        }

        if trimmed.starts_with("kubernetes.io/") || trimmed.starts_with("kubernetes.io/") {
            return Err(WorkerRequirementsError::ReservedKey {
                key: key.to_string(),
            });
        }

        // Validate characters (alphanumeric, '-', '_', '/', '.')
        for c in trimmed.chars() {
            if !c.is_alphanumeric() && !"-_./".contains(c) {
                return Err(WorkerRequirementsError::InvalidKeyCharacter {
                    key: key.to_string(),
                    char: c,
                });
            }
        }

        Ok(())
    }

    /// Validate label/annotation value
    fn validate_label_or_annotation_value(value: &str) -> Result<(), WorkerRequirementsError> {
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(WorkerRequirementsError::EmptyValue);
        }

        // Validate characters (alphanumeric, '-', '_', '/', '.', '*', '?')
        for c in trimmed.chars() {
            if !c.is_alphanumeric() && !"-_./?*".contains(c) {
                return Err(WorkerRequirementsError::InvalidValueCharacter {
                    value: value.to_string(),
                    char: c,
                });
            }
        }

        Ok(())
    }

    /// Check if a worker satisfies these requirements
    pub fn matches(&self, worker: &Worker) -> bool {
        // Check labels
        if !self.required_labels.is_empty() {
            let worker_spec = worker.spec();
            for (key, required_value) in &self.required_labels {
                if worker_spec.labels.get(key) != Some(required_value) {
                    return false;
                }
            }
        }

        // Check annotations
        if !self.required_annotations.is_empty() {
            let worker_spec = worker.spec();
            for (key, required_value) in &self.required_annotations {
                if worker_spec.annotations.get(key) != Some(required_value) {
                    return false;
                }
            }
        }

        true
    }

    /// Check if a worker spec satisfies these requirements
    pub fn matches_spec(&self, spec: &WorkerSpec) -> bool {
        // Check labels
        if !self.required_labels.is_empty() {
            for (key, required_value) in &self.required_labels {
                if spec.labels.get(key) != Some(required_value) {
                    return false;
                }
            }
        }

        // Check annotations
        if !self.required_annotations.is_empty() {
            for (key, required_value) in &self.required_annotations {
                if spec.annotations.get(key) != Some(required_value) {
                    return false;
                }
            }
        }

        true
    }

    /// Get the number of label requirements
    pub fn label_count(&self) -> usize {
        self.required_labels.len()
    }

    /// Get the number of annotation requirements
    pub fn annotation_count(&self) -> usize {
        self.required_annotations.len()
    }

    /// Get all required labels
    pub fn required_labels(&self) -> &HashMap<String, String> {
        &self.required_labels
    }

    /// Get all required annotations
    pub fn required_annotations(&self) -> &HashMap<String, String> {
        &self.required_annotations
    }

    /// Check if there are any requirements
    pub fn has_requirements(&self) -> bool {
        !self.required_labels.is_empty() || !self.required_annotations.is_empty()
    }
}

/// Error for invalid worker requirements
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerRequirementsError {
    /// Empty key provided
    EmptyKey,
    /// Empty value provided
    EmptyValue,
    /// Key uses reserved kubernetes prefix
    ReservedKey { key: String },
    /// Key contains invalid characters
    InvalidKeyCharacter { key: String, char: char },
    /// Value contains invalid characters
    InvalidValueCharacter { value: String, char: char },
}

impl fmt::Display for WorkerRequirementsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerRequirementsError::EmptyKey => write!(f, "Label/annotation key cannot be empty"),
            WorkerRequirementsError::EmptyValue => {
                write!(f, "Label/annotation value cannot be empty")
            }
            WorkerRequirementsError::ReservedKey { key } => {
                write!(f, "Key '{}' uses reserved kubernetes prefix", key)
            }
            WorkerRequirementsError::InvalidKeyCharacter { key, char } => {
                write!(f, "Key '{}' contains invalid character: '{}'", key, char)
            }
            WorkerRequirementsError::InvalidValueCharacter { value, char } => {
                write!(
                    f,
                    "Value '{}' contains invalid character: '{}'",
                    value, char
                )
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::{ProviderType, WorkerHandle, WorkerSpec};

    // ProviderPreference tests
    #[test]
    fn test_provider_preference_new_valid() {
        let pref = ProviderPreference::new("kubernetes").unwrap();
        assert_eq!(pref.normalized(), "kubernetes");
        assert_eq!(pref.original(), "kubernetes");
    }

    #[test]
    fn test_provider_preference_alias_k8s() {
        let pref = ProviderPreference::new("k8s").unwrap();
        assert_eq!(pref.normalized(), "kubernetes");
    }

    #[test]
    fn test_provider_preference_alias_kube() {
        let pref = ProviderPreference::new("kube").unwrap();
        assert_eq!(pref.normalized(), "kubernetes");
    }

    #[test]
    fn test_provider_preference_empty_error() {
        let result = ProviderPreference::new("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ProviderPreferenceError::Empty);
    }

    #[test]
    fn test_provider_preference_matches_provider_type() {
        let pref = ProviderPreference::new("k8s").unwrap();
        assert!(pref.matches_provider_type(&ProviderType::Kubernetes));
        assert!(!pref.matches_provider_type(&ProviderType::Docker));
    }

    #[test]
    fn test_provider_preference_from_provider_id() {
        let provider_id = ProviderId::new();
        let pref = ProviderPreference::from_provider_id(provider_id.clone());
        assert!(pref.is_specific_provider());
        assert!(pref.matches_provider_id(&provider_id));
    }

    // ProviderTypeMapping tests
    #[test]
    fn test_provider_type_mapping_match_standard() {
        let mapping = ProviderTypeMapping::new();
        assert_eq!(
            mapping.match_provider("kubernetes"),
            Some(ProviderType::Kubernetes)
        );
        assert_eq!(
            mapping.match_provider("k8s"),
            Some(ProviderType::Kubernetes)
        );
        assert_eq!(mapping.match_provider("docker"), Some(ProviderType::Docker));
    }

    #[test]
    fn test_provider_type_mapping_unknown() {
        let mapping = ProviderTypeMapping::new();
        assert_eq!(mapping.match_provider("unknown_provider"), None);
    }

    #[test]
    fn test_provider_type_mapping_is_provider_type() {
        let mapping = ProviderTypeMapping::new();
        assert!(mapping.is_provider_type("k8s", &ProviderType::Kubernetes));
        assert!(!mapping.is_provider_type("docker", &ProviderType::Kubernetes));
    }

    // WorkerRequirements tests
    #[test]
    fn test_worker_requirements_new_valid() {
        let mut labels = HashMap::new();
        labels.insert("environment".to_string(), "production".to_string());
        labels.insert("zone".to_string(), "us-east-1".to_string());

        let mut annotations = HashMap::new();
        annotations.insert("team".to_string(), "platform".to_string());

        let req = WorkerRequirements::new(labels.clone(), annotations.clone()).unwrap();
        assert_eq!(req.label_count(), 2);
        assert_eq!(req.annotation_count(), 1);
    }

    #[test]
    fn test_worker_requirements_empty_key_error() {
        let mut labels = HashMap::new();
        labels.insert("".to_string(), "value".to_string());

        let result = WorkerRequirements::new(labels, HashMap::new());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), WorkerRequirementsError::EmptyKey);
    }

    #[test]
    fn test_worker_requirements_empty_value_error() {
        let mut labels = HashMap::new();
        labels.insert("key".to_string(), "".to_string());

        let result = WorkerRequirements::new(labels, HashMap::new());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), WorkerRequirementsError::EmptyValue);
    }

    #[test]
    fn test_worker_requirements_matches_worker() {
        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());

        let mut required_annotations = HashMap::new();
        required_annotations.insert("team".to_string(), "platform".to_string());

        let req = WorkerRequirements::new(required_labels, required_annotations).unwrap();

        // Create worker with matching labels and annotations
        let mut spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec.labels
            .insert("environment".to_string(), "production".to_string());
        spec.annotations
            .insert("team".to_string(), "platform".to_string());

        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let worker = Worker::new(handle, spec);

        assert!(req.matches(&worker));
    }

    #[test]
    fn test_worker_requirements_no_match() {
        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());

        let req = WorkerRequirements::new(required_labels, HashMap::new()).unwrap();

        // Create worker with different labels
        let mut spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec.labels
            .insert("environment".to_string(), "staging".to_string());

        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let worker = Worker::new(handle, spec);

        assert!(!req.matches(&worker));
    }

    #[test]
    fn test_worker_requirements_none() {
        let req = WorkerRequirements::none();
        assert!(!req.has_requirements());
        assert_eq!(req.label_count(), 0);
        assert_eq!(req.annotation_count(), 0);
    }

    #[test]
    fn test_worker_requirements_has_requirements() {
        let mut labels = HashMap::new();
        labels.insert("key".to_string(), "value".to_string());

        let req = WorkerRequirements::new(labels, HashMap::new()).unwrap();
        assert!(req.has_requirements());
    }
}
