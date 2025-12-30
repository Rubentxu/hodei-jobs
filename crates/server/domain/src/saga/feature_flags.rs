//! Saga Feature Flags
//!
//! Feature flags para control gradual de la implementación del Saga pattern.
//! Permite shadow mode para comparar resultados entre implementación legacy y saga.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Feature flags para saga pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaFeatureFlags {
    /// Habilitar saga de aprovisionamiento
    pub provisioning_saga_enabled: bool,
    /// Habilitar saga de ejecución
    pub execution_saga_enabled: bool,
    /// Habilitar saga de recovery
    pub recovery_saga_enabled: bool,
    /// Shadow mode - ejecutar saga pero no usar resultado
    pub shadow_mode: bool,
    /// Timeout por defecto para sagas
    pub saga_timeout: Duration,
    /// Número máximo de sagas concurrentes
    pub max_concurrent_sagas: usize,
    /// Número máximo de steps concurrentes
    pub max_concurrent_steps: usize,
    /// Intervalo de cleanup para sagas completadas
    pub cleanup_interval: Duration,
}

impl Default for SagaFeatureFlags {
    fn default() -> Self {
        Self {
            provisioning_saga_enabled: false,
            execution_saga_enabled: false,
            recovery_saga_enabled: false,
            shadow_mode: true, // Por defecto en shadow mode
            saga_timeout: Duration::from_secs(300),
            max_concurrent_sagas: 100,
            max_concurrent_steps: 10,
            cleanup_interval: Duration::from_secs(3600),
        }
    }
}

impl SagaFeatureFlags {
    /// Crear feature flags para testing
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

    /// Crear feature flags para producción (shadow mode por defecto)
    pub fn production() -> Self {
        Self::default()
    }

    /// Verificar si una saga específica está habilitada
    pub fn is_saga_enabled(&self, saga_type: crate::saga::SagaType) -> bool {
        match saga_type {
            crate::saga::SagaType::Provisioning => self.provisioning_saga_enabled,
            crate::saga::SagaType::Execution => self.execution_saga_enabled,
            crate::saga::SagaType::Recovery => self.recovery_saga_enabled,
        }
    }

    /// Verificar si estamos en shadow mode
    pub fn is_shadow_mode(&self) -> bool {
        self.shadow_mode
    }
}

/// Versión atómica de feature flags para uso en tiempo de ejecución
#[derive(Debug, Clone)]
pub struct AtomicSagaFeatureFlags {
    inner: Arc<SagaFeatureFlags>,
}

impl Default for AtomicSagaFeatureFlags {
    fn default() -> Self {
        Self::new(SagaFeatureFlags::default())
    }
}

impl AtomicSagaFeatureFlags {
    /// Crear nuevos feature flags atómicos
    pub fn new(flags: SagaFeatureFlags) -> Self {
        Self {
            inner: Arc::new(flags),
        }
    }

    /// Actualizar feature flags (thread-safe)
    pub fn update(&self, f: impl FnOnce(&mut SagaFeatureFlags)) {
        let mut new_flags = (*self.inner).clone();
        f(&mut new_flags);
        self.inner = Arc::new(new_flags);
    }

    /// Obtener referencia a los flags actuales
    pub fn load(&self) -> &SagaFeatureFlags {
        &self.inner
    }

    /// Verificar si saga está habilitada
    pub fn is_saga_enabled(&self, saga_type: crate::saga::SagaType) -> bool {
        self.inner.is_saga_enabled(saga_type)
    }

    /// Verificar shadow mode
    pub fn is_shadow_mode(&self) -> bool {
        self.inner.shadow_mode
    }
}

/// Resultado de comparación entre saga y legacy
#[derive(Debug, Clone)]
pub struct SagaComparisonResult {
    /// Si los resultados coincidieron
    pub results_matched: bool,
    /// Resultado de la implementación saga
    pub saga_result: serde_json::Value,
    /// Resultado de la implementación legacy
    pub legacy_result: serde_json::Value,
    /// Diferencias encontradas
    pub discrepancies: Vec<Discrepancy>,
    /// Si estaba en shadow mode
    pub shadow_mode: bool,
    /// Timestamp de la comparación
    pub compared_at: chrono::DateTime<Utc>,
}

impl SagaComparisonResult {
    /// Crear resultado de comparación
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

    /// Encontrar diferencias entre dos valores JSON
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
                    let saga_val = saga_obj.get(key);
                    let legacy_val = legacy_obj.get(key);

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

    /// Loggear discrepancias encontradas
    pub fn log_discrepancies(&self, logger: &impl std::fmt::Display) {
        if !self.results_matched {
            tracing::warn!(
                target: "saga_comparison",
                "Discrepancies found between saga and legacy: {}",
                self
            );
        }
    }
}

/// Representa una diferencia entre dos resultados
#[derive(Debug, Clone)]
pub struct Discrepancy {
    pub path: String,
    pub saga_value: Option<serde_json::Value>,
    pub legacy_value: Option<serde_json::Value>,
}

impl std::fmt::Display for Discrepancy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Path: {}", self.path)?;
        if let Some(saga) = &self.saga_value {
            write!(f, ", Saga: {}", saga)?;
        }
        if let Some(legacy) = &self.legacy_value {
            write!(f, ", Legacy: {}", legacy)?;
        }
        Ok(())
    }
}

/// Shadow mode logger para registrar discrepancias
#[derive(Debug, Clone)]
pub struct ShadowModeLogger {
    log_saga_result: Arc<AtomicBool>,
    log_legacy_result: Arc<AtomicBool>,
    log_discrepancies: Arc<AtomicBool>,
}

impl ShadowModeLogger {
    pub fn new() -> Self {
        Self {
            log_saga_result: Arc::new(AtomicBool::new(true)),
            log_legacy_result: Arc::new(AtomicBool::new(true)),
            log_discrepancies: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Loggear resultado de saga en shadow mode
    pub fn log_saga_result(&self, result: &serde_json::Value) {
        if self.log_saga_result.load(Ordering::SeqCst) {
            tracing::debug!(target: "saga_shadow", "Saga result: {}", result);
        }
    }

    /// Loggear resultado legacy en shadow mode
    pub fn log_legacy_result(&self, result: &serde_json::Value) {
        if self.log_legacy_result.load(Ordering::SeqCst) {
            tracing::debug!(target: "saga_shadow", "Legacy result: {}", result);
        }
    }

    /// Loggear discrepancias
    pub fn log_discrepancies(&self, discrepancies: &[Discrepancy]) {
        if self.log_discrepancies.load(Ordering::SeqCst) && !discrepancies.is_empty() {
            tracing::warn!(target: "saga_shadow", "Discrepancies: {:?}", discrepancies);
        }
    }

    /// Loggear comparación completa
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

        // All disabled by default
        assert!(!flags.provisioning_saga_enabled);
        assert!(!flags.execution_saga_enabled);
        assert!(!flags.recovery_saga_enabled);

        // Shadow mode enabled by default
        assert!(flags.shadow_mode);
    }

    #[test]
    fn test_test_feature_flags() {
        let flags = SagaFeatureFlags::test();

        // All enabled for testing
        assert!(flags.provisioning_saga_enabled);
        assert!(flags.execution_saga_enabled);
        assert!(flags.recovery_saga_enabled);

        // Shadow mode disabled for testing
        assert!(!flags.shadow_mode);

        // Shorter timeouts for testing
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
    fn test_atomic_feature_flags() {
        let atomic = AtomicSagaFeatureFlags::default();
        assert!(!atomic.is_shadow_mode());

        atomic.update(|flags| {
            flags.shadow_mode = false;
            flags.execution_saga_enabled = true;
        });

        assert!(!atomic.is_shadow_mode());
        assert!(atomic.is_saga_enabled(crate::saga::SagaType::Execution));
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
        let saga = serde_json::json!({"status": "success", "id": "123"});
        let legacy = serde_json::json!({"status": "failed", "id": "456"});

        let comparison = SagaComparisonResult::new(saga, legacy, true);
        assert!(!comparison.results_matched);
        assert_eq!(comparison.discrepancies.len(), 2);
    }

    #[test]
    fn test_discrepancy_display() {
        let discrepancy = Discrepancy {
            path: "status".to_string(),
            saga_value: Some(serde_json::json!("success")),
            legacy_value: Some(serde_json::json!("failed")),
        };

        let display = discrepancy.to_string();
        assert!(display.contains("status"));
        assert!(display.contains("success"));
        assert!(display.contains("failed"));
    }

    #[test]
    fn test_shadow_mode_logger() {
        let logger = ShadowModeLogger::new();

        logger.log_saga_result(&serde_json::json!({"test": true}));
        logger.log_legacy_result(&serde_json::json!({"test": false}));
        logger.log_discrepancies(&[Discrepancy {
            path: "test".to_string(),
            saga_value: Some(serde_json::json!(true)),
            legacy_value: Some(serde_json::json!(false)),
        }]);
    }
}
