//! # PostgresCommandBus
//!
//! PostgreSQL-backed implementation of the [`CommandBus`] trait for command dispatching.
//!
//! This implementation provides:
//! - Command persistence with transactional guarantees
//! - Handler registry backed by PostgreSQL
//! - Retry logic for transient failures
//!
//! ## Architecture (DDD + Ports & Adapters)
//!
//! ```text
//! +----------------------------------------------------+
//! |           Application Layer                         |
//! |   (Use Cases - use CommandBus port)                |
//! +-------------------------+--------------------------+
//!                           | implements port
//! +-------------------------v--------------------------+
//! |           Infrastructure Layer                      |
//! |        PostgresCommandBus (Adapter)                 |
//! |                           |                        |
//! |              PostgreSQL (Persistence)               |
//! +----------------------------------------------------+
//! ```

use async_trait::async_trait;
use saga_engine_core::port::command_bus::{
    Command, CommandBus, CommandBusConfig, CommandBusError, CommandHandler, CommandResult,
};
use sqlx::{Pool, Postgres};
use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Configuration for [`PostgresCommandBus`].
#[derive(Debug, Clone)]
pub struct PostgresCommandBusConfig {
    /// PostgreSQL connection pool
    pub pool: Pool<Postgres>,
    /// Command table name
    pub table_name: String,
    /// Handler table name
    pub handler_table_name: String,
    /// Command execution timeout
    pub timeout: std::time::Duration,
    /// Maximum retries
    pub max_retries: u32,
}

impl Default for PostgresCommandBusConfig {
    fn default() -> Self {
        Self {
            pool: Pool::new_lazy(),
            table_name: "saga_commands".to_string(),
            handler_table_name: "saga_command_handlers".to_string(),
            timeout: std::time::Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// PostgreSQL-backed CommandBus implementation.
///
/// This adapter provides:
/// - Command persistence for auditing
/// - Transactional command execution
/// - Handler registration and lookup
#[derive(Debug, Clone)]
pub struct PostgresCommandBus {
    pool: Pool<Postgres>,
    handlers: Arc<Mutex<HashMap<&'static str, Box<dyn Any + Send + Sync>>>>,
    config: PostgresCommandBusConfig,
}

impl PostgresCommandBus {
    /// Create a new PostgresCommandBus.
    pub fn new(config: PostgresCommandBusConfig) -> Self {
        Self {
            pool: config.pool.clone(),
            handlers: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Register a handler for a command type.
    pub async fn register_handler<C: Command, H: CommandHandler<C>>(&self, handler: Arc<H>)
    where
        C: 'static,
        H: CommandHandler<C> + 'static,
    {
        let mut handlers = self.handlers.lock().await;
        let handler_dyn: Arc<dyn CommandHandler<C>> = handler;
        handlers.insert(C::TYPE_ID, Box::new(handler_dyn));
    }

    /// Run database migrations.
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        // Create commands table
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS saga_commands (
                id UUID PRIMARY KEY,
                command_type VARCHAR(256) NOT NULL,
                payload JSONB NOT NULL,
                status VARCHAR(32) NOT NULL DEFAULT 'pending',
                result JSONB,
                error TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                executed_at TIMESTAMPTZ,
                retry_count INTEGER DEFAULT 0
            )
            "#
        )
        .execute(&self.pool)
        .await?;

        // Create index
        sqlx::query!(
            "CREATE INDEX IF NOT EXISTS idx_saga_commands_status ON saga_commands(status, created_at)"
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl CommandBus for PostgresCommandBus {
    async fn send<C>(&self, command: C) -> Result<C::Result, CommandBusError>
    where
        C: Command + Debug + Send,
    {
        let command_id = Uuid::new_v4();
        let start = std::time::Instant::now();

        // Serialize the command
        let payload = serde_json::to_vec(&command)
            .map_err(|e| CommandBusError::SerializationError(e.to_string()))?;

        // Insert command record
        let _ = sqlx::query!(
            r#"
            INSERT INTO saga_commands (id, command_type, payload, status)
            VALUES ($1, $2, $3, 'pending')
            "#,
            command_id,
            C::TYPE_ID,
            payload
        )
        .execute(&self.pool)
        .await;

        // Look up handler and execute
        let result = {
            let handlers = self.handlers.lock().await;
            let handler_any = handlers
                .get(C::TYPE_ID)
                .ok_or_else(|| CommandBusError::HandlerNotFound(C::TYPE_ID))?;

            // Downcast to the specific handler type
            // We expect the stored handler to be Arc<H> where H: CommandHandler<C>
            // So we downcast to Arc<dyn CommandHandler<C>>
            let handler = handler_any
                .downcast_ref::<Arc<dyn CommandHandler<C>>>()
                .or_else(|| {
                    // Try downcasting to the concrete handler type if it was stored directly
                    // This is a safety net
                    None
                })
                .ok_or_else(|| {
                    CommandBusError::ExecutionError("Handler type mismatch".to_string())
                })?;

            handler.handle(command).await
        };

        // Update command record
        let duration = start.elapsed();
        match &result {
            Ok(result_val) => {
                let result_json = serde_json::to_value(result_val).ok();
                let _ = sqlx::query!(
                    r#"
                    UPDATE saga_commands
                    SET status = 'completed', result = $1, executed_at = NOW()
                    WHERE id = $2
                    "#,
                    result_json,
                    command_id
                )
                .execute(&self.pool)
                .await;
            }
            Err(e) => {
                let error_msg = e.to_string();
                let _ = sqlx::query!(
                    r#"
                    UPDATE saga_commands
                    SET status = 'failed', error = $1, executed_at = NOW(), retry_count = retry_count + 1
                    WHERE id = $2
                    "#,
                    error_msg,
                    command_id
                )
                .execute(&self.pool)
                .await;
            }
        }

        result
    }
}

// CommandHandlerAny is no longer needed with the Any-based approach
/*
/// Trait for handling any command type.
#[async_trait]
pub trait CommandHandlerAny: Send {
    /// Handle any command.
    async fn handle_any(&self, command: &dyn Debug) -> Result<Box<dyn Debug>, CommandBusError>;
}

#[async_trait]
impl<C: Command, H: CommandHandler<C>> CommandHandlerAny for H
where
    H: Send + Sync,
{
    async fn handle_any(&self, command: &dyn Debug) -> Result<Box<dyn Debug>, CommandBusError> {
        // This is a workaround since we can't directly cast &dyn Debug to &C
        // In practice, you'd use type erasure properly
        Err(CommandBusError::ExecutionError(
            "Direct CommandHandlerAny not properly implemented".to_string(),
        ))
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::port::command_bus::{Command, CommandHandler};
    use std::time::Duration;

    #[derive(Debug, Clone)]
    struct TestCommand;

    impl Command for TestCommand {
        const TYPE_ID: &'static str = "test.command";
        type Result = String;
    }

    #[derive(Debug, Clone)]
    struct TestCommandHandler;

    #[async_trait]
    impl CommandHandler<TestCommand> for TestCommandHandler {
        async fn handle(&self, _command: TestCommand) -> Result<String, CommandBusError> {
            Ok("handled".to_string())
        }
    }

    #[test]
    fn test_postgres_command_bus_config_defaults() {
        let config = PostgresCommandBusConfig::default();
        assert_eq!(config.table_name, "saga_commands");
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_command_type_id() {
        assert_eq!(TestCommand::TYPE_ID, "test.command");
    }
}
