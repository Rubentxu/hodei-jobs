//! # NATS JetStream Initialization
//!
//! This module provides automatic initialization and validation of NATS JetStream streams
//! during server startup, similar to database migrations.
//!
//! Streams are created idempotently - running multiple times won't create duplicates.

use async_nats::jetstream;
use async_nats::jetstream::stream::{Config as StreamConfig, StorageType};
use std::sync::Arc;
use tracing::{error, info, warn};

/// NATS stream configuration for automatic initialization
#[derive(Debug, Clone)]
pub struct NatsStreamConfig {
    /// Stream name (without prefix)
    pub name: String,
    /// Subject patterns to bind to this stream
    pub subjects: Vec<String>,
    /// Description for documentation
    pub description: String,
    /// Maximum number of messages (-1 = unlimited)
    pub max_messages: i64,
    /// Maximum bytes (-1 = unlimited)
    pub max_bytes: i64,
    /// Storage type
    pub storage: StorageType,
}

impl NatsStreamConfig {
    /// Full stream name with default prefix
    pub fn full_name(&self) -> String {
        format!("HODEI_{}", self.name)
    }
}

/// All NATS streams required by the server
pub const REQUIRED_STREAMS: &[NatsStreamConfig] = &[
    NatsStreamConfig {
        name: "EVENTS".to_string(),
        subjects: vec!["hodei.events.>".to_string()],
        description: "Persistent stream for all domain events".to_string(),
        max_messages: 2_000_000,
        max_bytes: 2 * 1024 * 1024 * 1024, // 2GB
        storage: StorageType::File,
    },
    NatsStreamConfig {
        name: "COMMANDS".to_string(),
        subjects: vec!["hodei.commands.>".to_string()],
        description: "Persistent stream for Hodei commands".to_string(),
        max_messages: 1_000_000,
        max_bytes: 1024 * 1024 * 1024, // 1GB
        storage: StorageType::File,
    },
    NatsStreamConfig {
        name: "TASKS".to_string(),
        subjects: vec!["saga.tasks.>".to_string()],
        description: "Persistent stream for saga tasks".to_string(),
        max_messages: 500_000,
        max_bytes: 512 * 1024 * 1024, // 512MB
        storage: StorageType::File,
    },
];

/// Initialize all required NATS JetStream streams
///
/// This function should be called during server startup, before any consumers are started.
/// It is idempotent - running multiple times is safe and won't cause errors.
///
/// # Errors
/// Returns an error if any stream cannot be created or verified
pub async fn initialize_nats_streams(
    nats_client: &async_nats::Client,
) -> Result<(), anyhow::Error> {
    let jetstream = jetstream::new(nats_client.clone());

    info!("🔧 Initializing NATS JetStream streams...");

    let mut all_ok = true;

    for stream_config in REQUIRED_STREAMS {
        match ensure_stream(&jetstream, stream_config).await {
            Ok(true) => {
                info!(
                    stream = stream_config.full_name(),
                    "✅ Stream created successfully"
                );
            }
            Ok(false) => {
                info!(
                    stream = stream_config.full_name(),
                    "✓ Stream already exists"
                );
            }
            Err(e) => {
                error!(
                    stream = stream_config.full_name(),
                    error = %e,
                    "❌ Failed to create/verify stream"
                );
                all_ok = false;
            }
        }
    }

    if !all_ok {
        return Err(anyhow::anyhow!(
            "Failed to initialize one or more NATS streams"
        ));
    }

    info!("✅ All NATS JetStream streams initialized successfully");
    Ok(())
}

/// Ensure a stream exists, creating it if necessary
///
/// Returns `Ok(true)` if stream was created, `Ok(false)` if it already existed
async fn ensure_stream(
    jetstream: &jetstream::Context,
    config: &NatsStreamConfig,
) -> Result<bool, anyhow::Error> {
    let full_name = config.full_name();
    let stream_config = StreamConfig {
        name: full_name.clone(),
        subjects: config.subjects.clone(),
        description: Some(config.description.clone()),
        max_messages: if config.max_messages > 0 {
            Some(config.max_messages as u64)
        } else {
            None
        },
        max_bytes: if config.max_bytes > 0 {
            Some(config.max_bytes as u64)
        } else {
            None
        },
        storage: config.storage,
        ..Default::default()
    };

    match jetstream.get_stream(&full_name).await {
        Ok(stream) => {
            // Stream exists, check if we need to update it
            let info = stream.info().await?;
            info!(
                stream = full_name,
                messages = info.messages,
                bytes = info.bytes,
                "Stream exists"
            );
            Ok(false) // Already exists
        }
        Err(_) => {
            // Stream doesn't exist, create it
            info!(stream = full_name, "Creating stream...");
            jetstream.create_stream(stream_config).await?;
            Ok(true) // Created
        }
    }
}

/// Validate that all required NATS streams exist
///
/// This is a lighter check than initialization - it just verifies streams exist
/// without creating them. Useful for health checks or readiness probes.
///
/// # Errors
/// Returns an error if any stream is missing
pub async fn validate_nats_streams(nats_client: &async_nats::Client) -> Result<(), anyhow::Error> {
    let jetstream = jetstream::new(nats_client.clone());

    for stream_config in REQUIRED_STREAMS {
        let full_name = stream_config.full_name();

        if jetstream.get_stream(&full_name).await.is_err() {
            return Err(anyhow::anyhow!(
                "Required NATS stream '{}' does not exist",
                full_name
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_config_full_name() {
        let config = NatsStreamConfig {
            name: "EVENTS".to_string(),
            subjects: vec!["hodei.events.>".to_string()],
            description: "Test stream".to_string(),
            max_messages: 1000,
            max_bytes: 1024,
            storage: StorageType::File,
        };

        assert_eq!(config.full_name(), "HODEI_EVENTS");
    }
}
