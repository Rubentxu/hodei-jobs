//! # NATS JetStream Initialization
//!
//! Automatic initialization of NATS JetStream streams during server startup.
//! Ensures all required streams exist before starting consumers.

use async_nats::jetstream;
use async_nats::jetstream::stream::{Config as StreamConfig, StorageType};
use tracing::info;

/// NATS stream configuration with static string slices for const initialization
#[derive(Debug, Clone)]
pub struct NatsStreamConfig {
    pub name: &'static str,
    pub subjects: &'static [&'static str],
    pub description: &'static str,
    pub max_messages: i64,
    pub max_bytes: i64,
    pub storage: StorageType,
}

impl NatsStreamConfig {
    /// Returns the full stream name with HODEI_ prefix
    #[inline]
    pub fn full_name(&self) -> String {
        format!("HODEI_{}", self.name)
    }

    /// Converts to owned types for JetStream Config
    pub fn to_stream_config(&self) -> StreamConfig {
        let full_name = self.full_name();
        StreamConfig {
            name: full_name.clone(),
            subjects: self.subjects.iter().copied().map(String::from).collect(),
            description: Some(self.description.to_string()),
            max_messages: self.max_messages,
            max_bytes: self.max_bytes,
            storage: self.storage,
            ..Default::default()
        }
    }
}

/// All streams required by the server
pub const REQUIRED_STREAMS: &[NatsStreamConfig] = &[
    NatsStreamConfig {
        name: "EVENTS",
        subjects: &["hodei.events.>"],
        description: "Domain events stream",
        max_messages: 2_000_000,
        max_bytes: 2 * 1024 * 1024 * 1024,
        storage: StorageType::File,
    },
    NatsStreamConfig {
        name: "COMMANDS",
        subjects: &["hodei.commands.>"],
        description: "Commands stream",
        max_messages: 1_000_000,
        max_bytes: 1024 * 1024 * 1024,
        storage: StorageType::File,
    },
    // Note: SAGA_TASKS stream is managed by saga-engine, not by the main server
];

/// Initialize all required NATS streams
///
/// Idempotent - safe to run multiple times
pub async fn initialize_nats_streams(
    nats_client: &async_nats::Client,
) -> Result<(), anyhow::Error> {
    let jetstream = jetstream::new(nats_client.clone());

    info!("🔧 Initializing NATS JetStream streams...");

    for stream in REQUIRED_STREAMS {
        let full_name = stream.full_name();
        let config = stream.to_stream_config();

        match jetstream.get_stream(&full_name).await {
            Ok(_) => {
                info!("✓ Stream {} exists", full_name);
            }
            Err(_) => {
                info!("Creating stream {}...", full_name);
                jetstream.create_stream(config).await?;
                info!("✅ Stream {} created", full_name);
            }
        }
    }

    info!("✅ All NATS streams initialized");
    Ok(())
}
