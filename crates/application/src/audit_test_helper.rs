//! Audit Test Helper - Utilidades para validar eventos en tests
//!
//! Proporciona helpers para verificar que los eventos de dominio esperados
//! fueron publicados durante la ejecución de casos de uso.

use hodei_jobs_domain::events::DomainEvent;
use std::sync::{Arc, Mutex};

/// Helper para capturar y validar eventos en tests
#[derive(Clone)]
pub struct AuditTestHelper {
    captured_events: Arc<Mutex<Vec<DomainEvent>>>,
}

impl AuditTestHelper {
    /// Crea un nuevo helper vacío
    pub fn new() -> Self {
        Self {
            captured_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Captura un evento
    pub fn capture(&self, event: DomainEvent) {
        self.captured_events.lock().unwrap().push(event);
    }

    /// Obtiene todos los eventos capturados
    pub fn events(&self) -> Vec<DomainEvent> {
        self.captured_events.lock().unwrap().clone()
    }

    /// Limpia los eventos capturados
    pub fn clear(&self) {
        self.captured_events.lock().unwrap().clear();
    }

    /// Verifica que se haya capturado al menos un evento del tipo especificado
    pub fn has_event_type(&self, event_type: &str) -> bool {
        self.captured_events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.event_type() == event_type)
    }

    /// Cuenta los eventos de un tipo específico
    pub fn count_event_type(&self, event_type: &str) -> usize {
        self.captured_events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.event_type() == event_type)
            .count()
    }

    /// Obtiene eventos de un tipo específico
    pub fn get_events_by_type(&self, event_type: &str) -> Vec<DomainEvent> {
        self.captured_events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.event_type() == event_type)
            .cloned()
            .collect()
    }

    /// Verifica que se hayan capturado exactamente los tipos de eventos especificados (en orden)
    pub fn assert_event_sequence(&self, expected_types: &[&str]) {
        let events = self.captured_events.lock().unwrap();
        let actual_types: Vec<&str> = events.iter().map(|e| e.event_type()).collect();
        
        assert_eq!(
            actual_types.len(),
            expected_types.len(),
            "Expected {} events but got {}: {:?}",
            expected_types.len(),
            actual_types.len(),
            actual_types
        );

        for (i, (actual, expected)) in actual_types.iter().zip(expected_types.iter()).enumerate() {
            assert_eq!(
                actual, expected,
                "Event at position {} was '{}' but expected '{}'",
                i, actual, expected
            );
        }
    }

    /// Verifica que se hayan capturado los tipos de eventos especificados (sin importar orden)
    pub fn assert_contains_event_types(&self, expected_types: &[&str]) {
        for expected in expected_types {
            assert!(
                self.has_event_type(expected),
                "Expected event type '{}' not found. Captured events: {:?}",
                expected,
                self.events().iter().map(|e| e.event_type()).collect::<Vec<_>>()
            );
        }
    }

    /// Verifica que un evento tenga el correlation_id esperado
    pub fn assert_correlation_id(&self, event_type: &str, expected_correlation_id: &str) {
        let events = self.get_events_by_type(event_type);
        assert!(
            !events.is_empty(),
            "No events of type '{}' found",
            event_type
        );

        let event = &events[0];
        assert_eq!(
            event.correlation_id().as_deref(),
            Some(expected_correlation_id),
            "Event '{}' has correlation_id {:?} but expected '{}'",
            event_type,
            event.correlation_id(),
            expected_correlation_id
        );
    }

    /// Verifica que un evento tenga el actor esperado
    pub fn assert_actor(&self, event_type: &str, expected_actor: &str) {
        let events = self.get_events_by_type(event_type);
        assert!(
            !events.is_empty(),
            "No events of type '{}' found",
            event_type
        );

        let event = &events[0];
        assert_eq!(
            event.actor().as_deref(),
            Some(expected_actor),
            "Event '{}' has actor {:?} but expected '{}'",
            event_type,
            event.actor(),
            expected_actor
        );
    }

    /// Verifica que no se hayan capturado eventos
    pub fn assert_no_events(&self) {
        let events = self.captured_events.lock().unwrap();
        assert!(
            events.is_empty(),
            "Expected no events but found {}: {:?}",
            events.len(),
            events.iter().map(|e| e.event_type()).collect::<Vec<_>>()
        );
    }

    /// Verifica que se haya capturado exactamente N eventos
    pub fn assert_event_count(&self, expected_count: usize) {
        let events = self.captured_events.lock().unwrap();
        assert_eq!(
            events.len(),
            expected_count,
            "Expected {} events but found {}: {:?}",
            expected_count,
            events.len(),
            events.iter().map(|e| e.event_type()).collect::<Vec<_>>()
        );
    }
}

impl Default for AuditTestHelper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use hodei_jobs_domain::job_execution::JobSpec;
    use hodei_jobs_domain::shared_kernel::JobId;

    fn create_job_created_event(correlation_id: Option<String>, actor: Option<String>) -> DomainEvent {
        DomainEvent::JobCreated {
            job_id: JobId::new(),
            spec: JobSpec::new(vec!["echo".to_string()]),
            occurred_at: Utc::now(),
            correlation_id,
            actor,
        }
    }

    fn create_job_cancelled_event() -> DomainEvent {
        DomainEvent::JobCancelled {
            job_id: JobId::new(),
            reason: Some("test".to_string()),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        }
    }

    #[test]
    fn test_capture_and_retrieve_events() {
        let helper = AuditTestHelper::new();
        
        helper.capture(create_job_created_event(None, None));
        helper.capture(create_job_cancelled_event());

        assert_eq!(helper.events().len(), 2);
    }

    #[test]
    fn test_has_event_type() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(None, None));

        assert!(helper.has_event_type("JobCreated"));
        assert!(!helper.has_event_type("JobCancelled"));
    }

    #[test]
    fn test_count_event_type() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(None, None));
        helper.capture(create_job_created_event(None, None));
        helper.capture(create_job_cancelled_event());

        assert_eq!(helper.count_event_type("JobCreated"), 2);
        assert_eq!(helper.count_event_type("JobCancelled"), 1);
    }

    #[test]
    fn test_assert_event_sequence() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(None, None));
        helper.capture(create_job_cancelled_event());

        helper.assert_event_sequence(&["JobCreated", "JobCancelled"]);
    }

    #[test]
    fn test_assert_contains_event_types() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(None, None));
        helper.capture(create_job_cancelled_event());

        helper.assert_contains_event_types(&["JobCancelled", "JobCreated"]);
    }

    #[test]
    fn test_assert_correlation_id() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(Some("test-corr-id".to_string()), None));

        helper.assert_correlation_id("JobCreated", "test-corr-id");
    }

    #[test]
    fn test_assert_actor() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(None, Some("test-actor".to_string())));

        helper.assert_actor("JobCreated", "test-actor");
    }

    #[test]
    fn test_clear() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(None, None));
        assert_eq!(helper.events().len(), 1);

        helper.clear();
        assert_eq!(helper.events().len(), 0);
    }

    #[test]
    fn test_assert_no_events() {
        let helper = AuditTestHelper::new();
        helper.assert_no_events();
    }

    #[test]
    fn test_assert_event_count() {
        let helper = AuditTestHelper::new();
        helper.capture(create_job_created_event(None, None));
        helper.capture(create_job_cancelled_event());

        helper.assert_event_count(2);
    }
}
