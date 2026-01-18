// Command Bus Module
//
// Provides Command Bus infrastructure for the Hodei Jobs Platform.
// Implements the Command pattern with handler registry and idempotency.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use uuid::Uuid;

/// Type of target for a command (analogous to AggregateType in events)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandTargetType {
    Job,
    Worker,
    Provider,
    Saga,
}

impl std::fmt::Display for CommandTargetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandTargetType::Job => write!(f, "JOB"),
            CommandTargetType::Worker => write!(f, "WORKER"),
            CommandTargetType::Provider => write!(f, "PROVIDER"),
            CommandTargetType::Saga => write!(f, "SAGA"),
        }
    }
}

impl std::str::FromStr for CommandTargetType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "JOB" => Ok(CommandTargetType::Job),
            "WORKER" => Ok(CommandTargetType::Worker),
            "PROVIDER" => Ok(CommandTargetType::Provider),
            "SAGA" => Ok(CommandTargetType::Saga),
            _ => Err(()),
        }
    }
}

/// Trait for command metadata containing tracing and context information
pub trait CommandMetadata: Send + Sync + Debug {
    fn trace_id(&self) -> Option<&str>;
    fn saga_id(&self) -> Option<&str>;
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>>;
    fn issuer(&self) -> Option<&str>;
}

/// Default implementation of command metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandMetadataDefault {
    pub trace_id: Option<String>,
    pub saga_id: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub issuer: Option<String>,
}

impl CommandMetadata for CommandMetadataDefault {
    fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    fn saga_id(&self) -> Option<&str> {
        self.saga_id.as_deref()
    }

    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.created_at
    }

    fn issuer(&self) -> Option<&str> {
        self.issuer.as_deref()
    }
}

impl CommandMetadataDefault {
    pub fn new() -> Self {
        Self {
            trace_id: Some(Uuid::new_v4().to_string()),
            saga_id: None,
            created_at: Some(chrono::Utc::now()),
            issuer: None,
        }
    }

    pub fn with_trace_id(trace_id: &str) -> Self {
        Self {
            trace_id: Some(trace_id.to_string()),
            saga_id: None,
            created_at: Some(chrono::Utc::now()),
            issuer: None,
        }
    }

    pub fn with_saga_id(mut self, saga_id: &str) -> Self {
        self.saga_id = Some(saga_id.to_string());
        self
    }

    pub fn with_issuer(mut self, issuer: &str) -> Self {
        self.issuer = Some(issuer.to_string());
        self
    }
}

/// Marker trait for all commands in the system.
/// Commands represent atomic business operations that can be dispatched through the Command Bus.
/// Commands must be Clone to support retry middleware.
pub trait Command: Debug + Clone + Send + Sync + 'static {
    /// The type returned by the handler when executing this command
    type Output: Send;

    /// Returns an idempotency key for this command.
    /// Uses `Cow<'_, str>` for zero-copy operations.
    fn idempotency_key(&self) -> Cow<'_, str>;

    /// Returns the target type for this command (e.g. Job, Worker).
    /// Used for routing and NATS subject construction.
    fn target_type(&self) -> CommandTargetType {
        CommandTargetType::Saga
    }

    /// Returns the command name (e.g. "AssignWorker").
    /// Used for routing and NATS subject construction.
    fn command_name(&self) -> Cow<'_, str> {
        let type_name = std::any::type_name::<Self>();
        // Extract just the struct name (last part after ::)
        let name = type_name.split("::").last().unwrap_or(type_name);
        Cow::Owned(name.to_string())
    }
}

/// Trait for command handlers.
#[async_trait]
pub trait CommandHandler<C: Command>: Send + Sync + 'static {
    /// Error type returned when command execution fails
    type Error: std::fmt::Debug + Send + Sync;

    /// Execute the command and return the result.
    async fn handle(&self, command: C) -> Result<C::Output, Self::Error>;
}

/// Trait for the Command Bus.
#[async_trait]
pub trait CommandBus: Debug + Send + Sync {
    /// Dispatch a command to its handler.
    async fn dispatch<C: Command>(&self, command: C) -> CommandResult<C::Output>;

    /// Register a handler for a specific command type.
    async fn register_handler<H, C>(&mut self, handler: H)
    where
        H: CommandHandler<C>,
        C: Command;
}

// Re-export submodules
pub mod bus;
pub mod erased;
pub mod error;
pub mod handler;
pub mod jobs;
pub mod middleware;
pub mod outbox;
pub mod registry;

pub use bus::{CommandBusConfig, InMemoryCommandBus};
pub use erased::{
    DynCommandBus, ErasedCommandBus, ErasedCommandBusExt, InMemoryErasedCommandBus, dispatch_erased,
};
pub use error::{CommandError, CommandResult};
pub use handler::HandlerBox;
pub use jobs::{
    MarkJobFailedCommand, MarkJobFailedError, MarkJobFailedHandler,
    ResumeFromManualInterventionCommand, ResumeFromManualInterventionError,
    ResumeFromManualInterventionHandler,
};
pub use middleware::{LoggingLayer, RetryLayer, TelemetryLayer};
pub use outbox::{
    CommandOutboxError, CommandOutboxInsert, CommandOutboxRecord, CommandOutboxRelay,
    CommandOutboxRepository, CommandOutboxStats, CommandOutboxStatus, OutboxCommandBus,
    OutboxCommandBusExt,
};
pub use registry::{HandlerRegistry, InMemoryHandlerStorage};
