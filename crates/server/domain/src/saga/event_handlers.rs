//! Reactive Event Handlers for Saga Pattern - EPIC-46 GAP-10/11
//!
//! This module provides a declarative, reactive event handling system for sagas.
//! Event handlers are pure functions that respond to saga lifecycle events,
//! enabling side-effect isolation and testability.

use super::types::{SagaContext, SagaId, SagaType};
use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;

/// Trait for saga event handlers.
///
/// Event handlers are pure functions that transform events into actions.
/// They don't execute side effects directly - they return descriptions
/// of actions to be taken by the infrastructure layer.
#[async_trait]
pub trait SagaEventHandler<Event: Debug + Clone + Send + Sync>: Send + Sync + Debug {
    /// The type of output this handler produces
    type Output: Send + Sync;

    /// Handles a saga event and returns actions to perform.
    async fn handle(
        &self,
        event: Event,
        saga_id: SagaId,
        saga_type: SagaType,
        context: &SagaContext,
    ) -> Vec<Self::Output>;
}

/// Represents an action to be taken by the infrastructure layer.
#[derive(Debug, Clone)]
pub enum SagaAction {
    /// Update metrics
    RecordMetric {
        metric_name: String,
        value: f64,
        tags: Vec<String>,
    },

    /// Send notification
    SendNotification {
        notification_type: NotificationType,
        target: String,
        message: String,
    },

    /// Log a message
    Log {
        level: LogLevel,
        message: String,
        saga_id: Option<SagaId>,
    },
}

/// Types of notifications that can be sent
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationType {
    JobCompleted,
    JobFailed,
    WorkerHealthAlert,
    SystemAlert,
}

/// Log levels for saga actions
#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Event types that can be handled in saga lifecycle
#[derive(Debug, Clone)]
pub enum SagaLifecycleEvent {
    /// Saga has started execution
    SagaStarted,
    /// A step has completed
    StepCompleted { step_name: String },
    /// A step has failed
    StepFailed {
        step_name: String,
        error: String,
        will_compensate: bool,
    },
    /// Compensation has started for a step
    CompensationStarted { step_name: String },
    /// Compensation has completed for a step
    CompensationCompleted { step_name: String },
    /// Saga has completed successfully
    SagaCompleted { duration_ms: u64 },
    /// Saga has failed and compensation is complete
    SagaFailed {
        last_completed_step: Option<String>,
        duration_ms: u64,
    },
}

/// Result of handling an event
#[derive(Debug)]
pub enum HandlerResult<T = SagaAction> {
    /// Handler produced actions
    Actions(Vec<T>),
    /// Handler decided to skip further processing
    Continue,
    /// Handler encountered an error
    Error(String),
}

impl<T> HandlerResult<T> {
    /// Convert to actions, defaulting to empty
    pub fn into_actions(self) -> Vec<T> {
        match self {
            HandlerResult::Actions(actions) => actions,
            HandlerResult::Continue => Vec::new(),
            HandlerResult::Error(_) => Vec::new(),
        }
    }
}

/// Registry of saga event handlers
#[derive(Debug)]
pub struct SagaEventHandlerRegistry<H: SagaEventHandler<SagaLifecycleEvent>> {
    handlers: Vec<Arc<H>>,
}

impl<H: SagaEventHandler<SagaLifecycleEvent>> SagaEventHandlerRegistry<H> {
    /// Creates a new empty registry
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Registers a handler
    pub fn register(&mut self, handler: Arc<H>) {
        self.handlers.push(handler);
    }

    /// Handles a lifecycle event by invoking all registered handlers
    pub async fn handle_event(
        &self,
        event: SagaLifecycleEvent,
        saga_id: &SagaId,
        saga_type: SagaType,
        context: &SagaContext,
    ) -> Vec<H::Output> {
        let mut results = Vec::new();

        for handler in &self.handlers {
            let handler_results = handler
                .handle(event.clone(), saga_id.clone(), saga_type, context)
                .await;
            results.extend(handler_results);
        }

        results
    }
}

impl<H: SagaEventHandler<SagaLifecycleEvent>> Default for SagaEventHandlerRegistry<H> {
    fn default() -> Self {
        Self::new()
    }
}

/// A handler that records metrics for saga lifecycle events
#[derive(Debug, Default)]
pub struct MetricsRecordingHandler;

impl MetricsRecordingHandler {
    /// Creates a new metrics recording handler
    pub fn new(_saga_type: SagaType) -> Self {
        Self
    }
}

#[async_trait]
impl SagaEventHandler<SagaLifecycleEvent> for MetricsRecordingHandler {
    type Output = SagaAction;

    async fn handle(
        &self,
        event: SagaLifecycleEvent,
        saga_id: SagaId,
        saga_type: SagaType,
        _context: &SagaContext,
    ) -> Vec<SagaAction> {
        match event {
            SagaLifecycleEvent::SagaCompleted { duration_ms: _ } => {
                vec![SagaAction::RecordMetric {
                    metric_name: "saga.completed".to_string(),
                    value: 1.0,
                    tags: vec![format!("type={}", saga_type.as_str())],
                }]
            }
            SagaLifecycleEvent::SagaFailed { .. } => vec![SagaAction::RecordMetric {
                metric_name: "saga.failed".to_string(),
                value: 1.0,
                tags: vec![format!("type={}", saga_type.as_str())],
            }],
            SagaLifecycleEvent::StepFailed {
                step_name,
                will_compensate,
                ..
            } => vec![
                SagaAction::RecordMetric {
                    metric_name: "saga.step.failed".to_string(),
                    value: 1.0,
                    tags: vec![
                        format!("type={}", saga_type.as_str()),
                        format!("step={}", step_name),
                        format!("compensation={}", will_compensate),
                    ],
                },
                SagaAction::Log {
                    level: LogLevel::Warn,
                    message: format!("Step '{}' failed in saga {}", step_name, saga_id),
                    saga_id: Some(saga_id),
                },
            ],
            _ => Vec::new(),
        }
    }
}

/// Handler that logs saga lifecycle events
#[derive(Debug)]
pub struct LoggingHandler;

impl LoggingHandler {
    /// Creates a new logging handler
    pub fn new() -> Self {
        Self
    }
}

impl Default for LoggingHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SagaEventHandler<SagaLifecycleEvent> for LoggingHandler {
    type Output = SagaAction;

    async fn handle(
        &self,
        event: SagaLifecycleEvent,
        saga_id: SagaId,
        saga_type: SagaType,
        _context: &SagaContext,
    ) -> Vec<SagaAction> {
        let (level, message) = match event {
            SagaLifecycleEvent::SagaStarted => (
                LogLevel::Info,
                format!("Saga {} ({}) started", saga_id, saga_type.as_str()),
            ),
            SagaLifecycleEvent::SagaCompleted { duration_ms } => (
                LogLevel::Info,
                format!(
                    "Saga {} ({}) completed in {}ms",
                    saga_id,
                    saga_type.as_str(),
                    duration_ms
                ),
            ),
            SagaLifecycleEvent::SagaFailed {
                last_completed_step: _,
                duration_ms,
            } => (
                LogLevel::Error,
                format!(
                    "Saga {} ({}) failed after {}ms",
                    saga_id,
                    saga_type.as_str(),
                    duration_ms
                ),
            ),
            SagaLifecycleEvent::StepFailed {
                step_name,
                error: _,
                will_compensate,
            } => (
                LogLevel::Warn,
                format!(
                    "Step '{}' failed in saga {} - will compensate: {}",
                    step_name, saga_id, will_compensate
                ),
            ),
            SagaLifecycleEvent::CompensationStarted { step_name } => (
                LogLevel::Info,
                format!("Compensating step '{}' in saga {}", step_name, saga_id),
            ),
            _ => return Vec::new(),
        };

        vec![SagaAction::Log {
            level,
            message,
            saga_id: Some(saga_id),
        }]
    }
}

/// Builder for saga event handlers
#[derive(Debug)]
pub struct SagaEventHandlerBuilder<H: SagaEventHandler<SagaLifecycleEvent>> {
    handlers: Vec<Arc<H>>,
}

impl<H: SagaEventHandler<SagaLifecycleEvent>> SagaEventHandlerBuilder<H> {
    /// Creates a new handler builder
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Adds a handler to the registry
    pub fn with_handler(mut self, handler: Arc<H>) -> Self {
        self.handlers.push(handler);
        self
    }

    /// Builds the handler registry
    pub fn build(self) -> SagaEventHandlerRegistry<H> {
        SagaEventHandlerRegistry {
            handlers: self.handlers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_logging_handler_saga_started() {
        let handler = LoggingHandler::new();
        let saga_id = SagaId::new();
        let saga_type = SagaType::Execution;
        let context = SagaContext::new(saga_id.clone(), saga_type.clone(), None, None);

        let actions = handler
            .handle(
                SagaLifecycleEvent::SagaStarted,
                saga_id.clone(),
                saga_type,
                &context,
            )
            .await;

        assert_eq!(actions.len(), 1);
        if let SagaAction::Log {
            level,
            message,
            saga_id: _id,
        } = &actions[0]
        {
            assert_eq!(*level, LogLevel::Info);
            assert!(message.contains("started"));
        } else {
            panic!("Expected Log action");
        }
    }

    #[tokio::test]
    async fn test_metrics_handler_saga_completed() {
        let handler = MetricsRecordingHandler::new(SagaType::Execution);
        let saga_id = SagaId::new();
        let saga_type = SagaType::Execution;
        let context = SagaContext::new(saga_id.clone(), saga_type.clone(), None, None);

        let actions = handler
            .handle(
                SagaLifecycleEvent::SagaCompleted { duration_ms: 100 },
                saga_id,
                saga_type,
                &context,
            )
            .await;

        assert_eq!(actions.len(), 1);
        if let SagaAction::RecordMetric {
            metric_name,
            value,
            tags,
        } = &actions[0]
        {
            assert_eq!(metric_name, "saga.completed");
            assert_eq!(*value, 1.0);
            assert!(tags.contains(&"type=EXECUTION".to_string()));
        } else {
            panic!("Expected RecordMetric action");
        }
    }

    #[tokio::test]
    async fn test_handler_registry() {
        let registry: SagaEventHandlerRegistry<LoggingHandler> = SagaEventHandlerRegistry::new();

        // Verify registry can be created
        assert!(true, "Registry created successfully");
    }

    #[test]
    fn test_connectivity_status_variants() {
        let _ = SagaLifecycleEvent::SagaStarted;
        let _ = SagaLifecycleEvent::SagaCompleted { duration_ms: 100 };
        assert!(true);
    }
}
