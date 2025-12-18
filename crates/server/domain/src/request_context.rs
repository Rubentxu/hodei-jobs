//! Request Context - Propagación de contexto para trazabilidad
//!
//! Proporciona un contexto inmutable que se propaga a través de las capas
//! de la aplicación para mantener correlation_id y actor en todos los eventos.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Contexto de request para propagación de trazabilidad
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// ID de correlación para trazar operaciones relacionadas
    correlation_id: String,
    /// Actor que inició la operación (usuario, sistema, etc.)
    actor: Option<String>,
    /// Timestamp de inicio de la operación
    started_at: chrono::DateTime<chrono::Utc>,
}

impl RequestContext {
    /// Crea un nuevo contexto con correlation_id generado automáticamente
    pub fn new() -> Self {
        Self {
            correlation_id: Uuid::new_v4().to_string(),
            actor: None,
            started_at: chrono::Utc::now(),
        }
    }

    /// Crea un contexto con un correlation_id específico
    pub fn with_correlation_id(correlation_id: impl Into<String>) -> Self {
        Self {
            correlation_id: correlation_id.into(),
            actor: None,
            started_at: chrono::Utc::now(),
        }
    }

    /// Crea un contexto con correlation_id y actor
    pub fn with_actor(correlation_id: impl Into<String>, actor: impl Into<String>) -> Self {
        Self {
            correlation_id: correlation_id.into(),
            actor: Some(actor.into()),
            started_at: chrono::Utc::now(),
        }
    }

    /// Builder: establece el actor
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Obtiene el correlation_id
    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    /// Obtiene el correlation_id como Option<String> para uso en eventos
    pub fn correlation_id_owned(&self) -> Option<String> {
        Some(self.correlation_id.clone())
    }

    /// Obtiene el actor
    pub fn get_actor(&self) -> Option<&str> {
        self.actor.as_deref()
    }

    /// Obtiene el actor como Option<String> para uso en eventos
    pub fn actor_owned(&self) -> Option<String> {
        self.actor.clone()
    }

    /// Obtiene el timestamp de inicio
    pub fn started_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.started_at
    }

    /// Crea un contexto hijo que hereda correlation_id pero puede tener diferente actor
    pub fn child_context(&self) -> Self {
        Self {
            correlation_id: self.correlation_id.clone(),
            actor: self.actor.clone(),
            started_at: chrono::Utc::now(),
        }
    }

    /// Crea un contexto hijo con un actor diferente
    pub fn child_with_actor(&self, actor: impl Into<String>) -> Self {
        Self {
            correlation_id: self.correlation_id.clone(),
            actor: Some(actor.into()),
            started_at: chrono::Utc::now(),
        }
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RequestContext(correlation_id={}, actor={:?})",
            self.correlation_id, self.actor
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_context_generates_correlation_id() {
        let ctx = RequestContext::new();
        assert!(!ctx.correlation_id().is_empty());
        assert!(ctx.get_actor().is_none());
    }

    #[test]
    fn test_with_correlation_id() {
        let ctx = RequestContext::with_correlation_id("test-123");
        assert_eq!(ctx.correlation_id(), "test-123");
    }

    #[test]
    fn test_with_actor() {
        let ctx = RequestContext::with_actor("corr-456", "user@example.com");
        assert_eq!(ctx.correlation_id(), "corr-456");
        assert_eq!(ctx.get_actor(), Some("user@example.com"));
    }

    #[test]
    fn test_builder_pattern() {
        let ctx = RequestContext::with_correlation_id("build-789")
            .actor("admin");
        assert_eq!(ctx.correlation_id(), "build-789");
        assert_eq!(ctx.get_actor(), Some("admin"));
    }

    #[test]
    fn test_child_context_inherits_correlation_id() {
        let parent = RequestContext::with_actor("parent-id", "parent-actor");
        let child = parent.child_context();
        
        assert_eq!(child.correlation_id(), "parent-id");
        assert_eq!(child.get_actor(), Some("parent-actor"));
    }

    #[test]
    fn test_child_with_different_actor() {
        let parent = RequestContext::with_actor("parent-id", "parent-actor");
        let child = parent.child_with_actor("child-actor");
        
        assert_eq!(child.correlation_id(), "parent-id");
        assert_eq!(child.get_actor(), Some("child-actor"));
    }

    #[test]
    fn test_owned_methods_for_events() {
        let ctx = RequestContext::with_actor("event-corr", "event-actor");
        
        assert_eq!(ctx.correlation_id_owned(), Some("event-corr".to_string()));
        assert_eq!(ctx.actor_owned(), Some("event-actor".to_string()));
    }

    #[test]
    fn test_display() {
        let ctx = RequestContext::with_actor("display-id", "display-actor");
        let display = format!("{}", ctx);
        assert!(display.contains("display-id"));
        assert!(display.contains("display-actor"));
    }
}
