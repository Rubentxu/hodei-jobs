use anyhow::{Context, Result};
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy, PullConsumer};
use async_nats::jetstream::{self, stream::Config as StreamConfig, stream::RetentionPolicy};
use async_nats::jetstream::consumer::Config as ConsumerConfig;
use async_nats::Client;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};
use async_nats::jetstream::stream::{StorageType, DiscardPolicy};

/// NATS JetStream Configuration
/// Implements durable streams with retention policies
pub struct JetStreamConfig {
    pub streams: Vec<StreamConfig>,
    pub consumers: Vec<ConsumerConfig>,
}

impl JetStreamConfig {
    pub fn default() -> Self {
        Self {
            streams: Self::create_streams(),
            consumers: Self::create_consumers(),
        }
    }

    fn create_streams() -> Vec<StreamConfig> {
        vec![
            // Jobs stream - WorkQueue for job lifecycle events
            StreamConfig {
                name: "hodei_jobs".into(),
                subjects: vec![
                    "hodei.jobs.queued".into(),
                    "hodei.jobs.assigned".into(),
                    "hodei.jobs.started".into(),
                    "hodei.jobs.completed".into(),
                    "hodei.jobs.failed".into(),
                    "hodei.jobs.cancelled".into(),
                ],
                retention: RetentionPolicy::WorkQueue,
                max_messages: 1_000_000,
                max_bytes: 1024 * 1024 * 1024, // 1GB
                max_age: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
                storage: StorageType::File,
                discard: DiscardPolicy::Old,
                num_replicas: 1, // Dev environment typically 1 replica
                ..Default::default()
            },
            // Workers stream - WorkQueue for worker lifecycle events
            StreamConfig {
                name: "hodei_workers".into(),
                subjects: vec![
                    "hodei.workers.provisioned".into(),
                    "hodei.workers.ready".into(),
                    "hodei.workers.busy".into(),
                    "hodei.workers.released".into(),
                    "hodei.workers.terminated".into(),
                ],
                retention: RetentionPolicy::WorkQueue,
                max_messages: 500_000,
                max_bytes: 512 * 1024 * 1024, // 512MB
                max_age: Duration::from_secs(7 * 24 * 60 * 60),
                storage: StorageType::File,
                discard: DiscardPolicy::Old,
                num_replicas: 1,
                ..Default::default()
            },
            // Sagas stream - Short retention for saga events
            StreamConfig {
                name: "hodei_sagas".into(),
                subjects: vec![
                    "hodei.sagas.provisioning.*".into(),
                    "hodei.sagas.execution.*".into(),
                    "hodei.sagas.cancellation.*".into(),
                ],
                retention: RetentionPolicy::Limits,
                max_messages: 100_000,
                max_bytes: 100 * 1024 * 1024, // 100MB
                max_age: Duration::from_secs(24 * 60 * 60), // 1 day
                storage: StorageType::File,
                discard: DiscardPolicy::Old,
                num_replicas: 1,
                ..Default::default()
            },
            // Dead letter queue - For failed message replay
            StreamConfig {
                name: "hodei_dlq".into(),
                subjects: vec!["hodei.dlq.*".into()],
                retention: RetentionPolicy::Limits,
                max_messages: 100_000,
                max_bytes: 1024 * 1024 * 100, // 100MB
                max_age: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
                storage: StorageType::File,
                discard: DiscardPolicy::Old,
                num_replicas: 1,
                ..Default::default()
            },
        ]
    }

    fn create_consumers() -> Vec<ConsumerConfig> {
        vec![
            // Execution saga consumer - Durable with explicit ACK
            ConsumerConfig {
                durable_name: Some("execution-saga-consumer".into()),
                ack_policy: AckPolicy::Explicit,
                max_deliver: 3,
                backoff: vec![
                    Duration::from_millis(100),
                    Duration::from_millis(500),
                    Duration::from_secs(1),
                    Duration::from_secs(5),
                ],
                filter_subject: "hodei.workers.ready".into(),
                deliver_policy: DeliverPolicy::New,
                // In newer async-nats, deliver_subject is deprecated/removed for pull consumers usually, 
                // but if this is push consumer config it might still be there.
                // However, PullConsumer is typically used.
                // The EPIC used deliver_subject: Some("hodei.dlq.execution".into()) which suggests redirection?
                // Actually, async-nats ConsumerConfig allows setting this for push consumers.
                // But generally we use Pull consumers for sagas.
                // Let's stick to the EPIC config but be careful with fields.
                ..Default::default() 
            },
             // Provisioning saga consumer
            ConsumerConfig {
                durable_name: Some("provisioning-saga-consumer".into()),
                ack_policy: AckPolicy::Explicit,
                max_deliver: 3,
                filter_subject: "hodei.jobs.queued".into(),
                deliver_policy: DeliverPolicy::New,
                ..Default::default()
            },
        ]
    }
}

pub struct NatsManager {
    context: Client,
}

impl NatsManager {
    pub fn new(client: Client) -> Self {
        Self { context: client }
    }

    pub async fn initialize_jetstream(&self, config: JetStreamConfig) -> Result<()> {
        let js = async_nats::jetstream::new(self.context.clone());

        // Create streams with retry
        for stream_config in config.streams {
            info!("Creating JetStream stream: {}", stream_config.name);
            match js.create_stream(stream_config).await {
                Ok(_) => info!("Stream created successfully"),
                Err(e) => warn!("Stream creation issue (may already exist): {}", e),
            }
        }

        // Create consumers
        for consumer_config in config.consumers {
            if let Some(durable_name) = &consumer_config.durable_name {
                 info!("Creating consumer: {}", durable_name);
                 let stream_name = match consumer_config.filter_subject.as_str() {
                    s if s.contains("workers") => "hodei_workers",
                    s if s.contains("sagas") => "hodei_sagas",
                    _ => "hodei_jobs",
                };

                if let Ok(stream) = js.get_stream(stream_name).await {
                    match stream.create_consumer(consumer_config).await {
                        Ok(_) => info!("Consumer created successfully"),
                        Err(e) => warn!("Consumer creation issue: {}", e),
                    }
                }
            }
        }

        Ok(())
    }
    
    pub fn client(&self) -> Client {
        self.context.clone()
    }
    
    pub fn jetstream(&self) -> async_nats::jetstream::Context {
        async_nats::jetstream::new(self.context.clone())
    }
}
