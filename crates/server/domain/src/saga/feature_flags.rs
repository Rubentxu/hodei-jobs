//! Saga Feature Flags
//!
//! Feature flags para control gradual de la implementación del Saga pattern.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Feature flags para saga pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaFeatureFlags {
    pub provisioning_saga_enabled: bool,
    pub execution_saga_enabled: bool,
    pub recovery_saga_enabled: bool,
    pub shadow_mode: bool,
    pub saga_timeout: Duration,
    pub max_concurrent_sagas: usize,
    pub max_concurrent_steps: usize,
    pub cleanup_interval: Duration,
}

impl Default for SagaFeatureFlags {
    fn default() -> Self {
        Self {
            provisioning_saga_enabled: false,
            execution_saga_enabled: false,
            recovery_saga_enabled: false,
            shadow_mode: true,
            saga_timeout: Duration::from_secs(300),
            max_concurrent_sagas: 100,
            max_concurrent_steps: 10,
            cleanup_interval: Duration::from_secs(3600),
        }
    }
}

impl SagaFeatureFlags {
    #[inline]
    pub fn test() -> Self {
        Self {
            provisioning_saga_enabled: true,
            execution_saga_enabled: true,
            recovery_saga_enabled: true,
            shadow_mode: false,
            saga_timeout: Duration::from_secs(30),
            max_concurrent_sagas: 10,
            max_concurrent_steps: 5,
            cleanup_interval: Duration::from_secs(60),
        }
    }

    #[inline]
    pub fn production() -> Self {
        Self::default()
    }

    #[inline]
    pub fn is_saga_enabled(&self, saga_type: crate::saga::SagaType) -> bool {
        match saga_type {
            crate::saga::SagaType::Provisioning => self.provisioning_saga_enabled,
            crate::saga::SagaType::Execution => self.execution_saga_enabled,
            crate::saga::SagaType::Recovery => self.recovery_saga_enabled,
        }
    }

    #[inline]
    pub fn is_shadow_mode(&self) -> bool {
        self.shadow_mode
    }
}

/// Versión thread-safe de feature flags con Mutex interior
#[derive(Debug)]
pub struct AtomicSagaFeatureFlags {
    inner: Arc<Mutex<SagaFeatureFlags>>,
}

impl Clone for AtomicSagaFeatureFlags {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for AtomicSagaFeatureFlags {
    fn default() -> Self {
        Self::new(SagaFeatureFlags::default())
    }
}

impl AtomicSagaFeatureFlags {
    #[inline]
    pub fn new(flags: SagaFeatureFlags) -> Self {
        Self {
            inner: Arc::new(Mutex::new(flags)),
        }
    }

    pub fn update(&self, f: impl FnOnce(&mut SagaFeatureFlags)) {
        let mut guard = self.inner.lock().unwrap();
        f(&mut *guard);
    }

    #[inline]
    pub fn load(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, SagaFeatureFlags>> {
        self.inner.lock()
    }

    #[inline]
    pub fn is_saga_enabled(&self, saga_type: crate::saga::SagaType) -> bool {
        self.inner.lock().unwrap().is_saga_enabled(saga_type)
    }

    #[inline]
    pub fn is_shadow_mode(&self) -> bool {
        self.inner.lock().unwrap().shadow_mode
    }
}

/// Resultado de comparación entre saga y legacy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaComparisonResult {
    pub results_matched: bool,
    pub saga_result: serde_json::Value,
    pub legacy_result: serde_json::Value,
    pub discrepancies: Vec<Discrepancy>,
    pub shadow_mode: bool,
    pub compared_at: DateTime<Utc>,
}

impl SagaComparisonResult {
    #[inline]
    pub fn new(
        saga_result: serde_json::Value,
        legacy_result: serde_json::Value,
        shadow_mode: bool,
    ) -> Self {
        let discrepancies = Self::find_discrepancies(&saga_result, &legacy_result);
        let results_matched = discrepancies.is_empty();

        Self {
            results_matched,
            saga_result,
            legacy_result,
            discrepancies,
            shadow_mode,
            compared_at: Utc::now(),
        }
    }

    fn find_discrepancies(
        saga: &serde_json::Value,
        legacy: &serde_json::Value,
    ) -> Vec<Discrepancy> {
        let mut discrepancies = Vec::new();
        Self::compare_values(saga, legacy, "", &mut discrepancies);
        discrepancies
    }

    fn compare_values(
        saga: &serde_json::Value,
        legacy: &serde_json::Value,
        path: &str,
        discrepancies: &mut Vec<Discrepancy>,
    ) {
        match (saga, legacy) {
            (serde_json::Value::Object(saga_obj), serde_json::Value::Object(legacy_obj)) => {
                for key in saga_obj.keys().chain(legacy_obj.keys()) {
                    let new_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    let saga_val = saga_obj.get(key.as_str());
                    let legacy_val = legacy_obj.get(key.as_str());

                    match (saga_val, legacy_val) {
                        (Some(s), Some(l)) => {
                            Self::compare_values(s, l, &new_path, discrepancies);
                        }
                        (Some(s), None) => {
                            discrepancies.push(Discrepancy {
                                path: new_path,
                                saga_value: Some(s.clone()),
                                legacy_value: None,
                            });
                        }
                        (None, Some(l)) => {
                            discrepancies.push(Discrepancy {
                                path: new_path,
                                saga_value: None,
                                legacy_value: Some(l.clone()),
                            });
                        }
                        (None, None) => {}
                    }
                }
            }
            (s, l) if s != l => {
                discrepancies.push(Discrepancy {
                    path: path.to_string(),
                    saga_value: Some(s.clone()),
                    legacy_value: Some(l.clone()),
                });
            }
            _ => {}
        }
    }

    #[inline]
    pub fn has_discrepancies(&self) -> bool {
        !self.discrepancies.is_empty()
    }
}

/// Representa una diferencia entre dos resultados
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discrepancy {
    pub path: String,
    pub saga_value: Option<serde_json::Value>,
    pub legacy_value: Option<serde_json::Value>,
}

/// Shadow mode logger
#[derive(Debug, Clone)]
pub struct ShadowModeLogger {
    log_saga_result: Arc<AtomicBool>,
    log_legacy_result: Arc<AtomicBool>,
    log_discrepancies: Arc<AtomicBool>,
}

impl ShadowModeLogger {
    #[inline]
    pub fn new() -> Self {
        Self {
            log_saga_result: Arc::new(AtomicBool::new(true)),
            log_legacy_result: Arc::new(AtomicBool::new(true)),
            log_discrepancies: Arc::new(AtomicBool::new(true)),
        }
    }

    #[inline]
    pub fn log_saga_result(&self, _result: &serde_json::Value) {
        if self.log_saga_result.load(Ordering::SeqCst) {
            tracing::debug!(target: "saga_shadow", "Saga result logged");
        }
    }

    #[inline]
    pub fn log_legacy_result(&self, _result: &serde_json::Value) {
        if self.log_legacy_result.load(Ordering::SeqCst) {
            tracing::debug!(target: "saga_shadow", "Legacy result logged");
        }
    }

    #[inline]
    pub fn log_discrepancies(&self, discrepancies: &[Discrepancy]) {
        if self.log_discrepancies.load(Ordering::SeqCst) && !discrepancies.is_empty() {
            tracing::warn!(target: "saga_shadow", "Discrepancies: {:?}", discrepancies);
        }
    }

    #[inline]
    pub fn log_comparison(&self, comparison: &SagaComparisonResult) {
        self.log_saga_result(&comparison.saga_result);
        self.log_legacy_result(&comparison.legacy_result);
        self.log_discrepancies(&comparison.discrepancies);
    }
}

impl Default for ShadowModeLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_feature_flags() {
        let flags = SagaFeatureFlags::default();
        assert!(!flags.provisioning_saga_enabled);
        assert!(!flags.execution_saga_enabled);
        assert!(!flags.recovery_saga_enabled);
        assert!(flags.shadow_mode);
    }

    #[test]
    fn test_test_feature_flags() {
        let flags = SagaFeatureFlags::test();
        assert!(flags.provisioning_saga_enabled);
        assert!(flags.execution_saga_enabled);
        assert!(flags.recovery_saga_enabled);
        assert!(!flags.shadow_mode);
        assert_eq!(flags.saga_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_is_saga_enabled() {
        let flags = SagaFeatureFlags::test();
        assert!(flags.is_saga_enabled(crate::saga::SagaType::Provisioning));
        assert!(flags.is_saga_enabled(crate::saga::SagaType::Execution));
        assert!(flags.is_saga_enabled(crate::saga::SagaType::Recovery));
    }

    #[test]
    fn test_atomic_feature_flags_update() {
        let flags = AtomicSagaFeatureFlags::default();
        assert!(!flags.is_shadow_mode());

        flags.update(|f| {
            f.shadow_mode = false;
            f.execution_saga_enabled = true;
        });

        assert!(!flags.is_shadow_mode());
        assert!(flags.is_saga_enabled(crate::saga::SagaType::Execution));
    }

    #[test]
    fn test_comparison_results_match() {
        let saga = serde_json::json!({"status": "success", "id": "123"});
        let legacy = serde_json::json!({"status": "success", "id": "123"});

        let comparison = SagaComparisonResult::new(saga, legacy, true);
        assert!(comparison.results_matched);
        assert!(comparison.discrepancies.is_empty());
    }

    #[test]
    fn test_comparison_results_differ() {
        let saga = serde_json::json!({"status": "success"});
        let legacy = serde_json::json!({"status": "failed"});

        let comparison = SagaComparisonResult::new(saga, legacy, true);
        assert!(!comparison.results_matched);
    }

    #[test]
    fn test_shadow_mode_logger() {
        let logger = ShadowModeLogger::new();
        logger.log_saga_result(&serde_json::json!({"test": true}));
        logger.log_legacy_result(&serde_json::json!({"test": false}));
        logger.log_discrepancies(&[]);
    }
}
