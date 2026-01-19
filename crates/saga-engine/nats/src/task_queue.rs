//! NATS TaskQueue implementation using async-nats Core.

use async_nats::Client;
use async_trait::async_trait;
use serde::Serialize;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use saga_engine_core::event::SagaId;
use saga_engine_core::port::task_queue::{Task, TaskId, TaskMessage, TaskQueue, TaskQueueError};

/// Configuration for [`NatsTaskQueue`].
#[derive(Debug, Clone, Default)]
pub struct NatsTaskQueueConfig {
    pub nats_url: String,
    pub subject_prefix: String,
}

impl Serialize for NatsTaskQueueConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("NatsTaskQueueConfig", 2)?;
        s.serialize_field("nats_url", &self.nats_url)?;
        s.serialize_field("subject_prefix", &self.subject_prefix)?;
        s.end()
    }
}

/// NATS-based TaskQueue implementation.
#[derive(Debug, Clone)]
pub struct NatsTaskQueue {
    client: Arc<Client>,
    config: NatsTaskQueueConfig,
}

impl NatsTaskQueue {
    pub async fn new(
        config: NatsTaskQueueConfig,
    ) -> Result<Self, TaskQueueError<Box<dyn std::error::Error + Send + Sync>>> {
        match async_nats::connect(&config.nats_url).await {
            Ok(client) => Ok(Self {
                client: Arc::new(client),
                config,
            }),
            Err(e) => Err(TaskQueueError::ConsumerCreation(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            )),
        }
    }
}

#[async_trait::async_trait]
impl TaskQueue for NatsTaskQueue {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn publish(
        &self,
        task: &Task,
        subject: &str,
    ) -> Result<TaskId, TaskQueueError<Self::Error>> {
        let payload = match serde_json::to_vec(task) {
            Ok(p) => p,
            Err(e) => {
                return Err(TaskQueueError::Publish(
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                ));
            }
        };
        match self
            .client
            .publish(subject.to_string(), payload.into())
            .await
        {
            Ok(()) => Ok(task.task_id.clone()),
            Err(e) => Err(TaskQueueError::Publish(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            )),
        }
    }

    async fn ensure_consumer(
        &self,
        _consumer_name: &str,
        _config: &saga_engine_core::port::task_queue::ConsumerConfig,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }

    async fn fetch(
        &self,
        _consumer_name: &str,
        _max_messages: u64,
        _timeout: Duration,
    ) -> Result<Vec<TaskMessage>, TaskQueueError<Self::Error>> {
        Ok(Vec::new())
    }

    async fn ack(&self, _message_id: &str) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }

    async fn nak(
        &self,
        _message_id: &str,
        _delay: Option<Duration>,
    ) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }

    async fn terminate(&self, _message_id: &str) -> Result<(), TaskQueueError<Self::Error>> {
        Ok(())
    }
}
