//! NATS SignalDispatcher implementation using async-nats.

use async_nats::Client;
use async_trait::async_trait;
use serde::Serialize;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use saga_engine_core::event::SagaId;
use saga_engine_core::port::signal_dispatcher::{
    SignalDispatcher, SignalDispatcherError, SignalNotification, SignalStream, SignalType,
};

/// Configuration for [`NatsSignalDispatcher`].
#[derive(Debug, Clone, Default)]
pub struct NatsSignalDispatcherConfig {
    pub nats_url: String,
    pub subject_prefix: String,
    pub subscription_capacity: usize,
}

impl Serialize for NatsSignalDispatcherConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("NatsSignalDispatcherConfig", 3)?;
        s.serialize_field("nats_url", &self.nats_url)?;
        s.serialize_field("subject_prefix", &self.subject_prefix)?;
        s.serialize_field("subscription_capacity", &self.subscription_capacity)?;
        s.end()
    }
}

/// NATS-based SignalDispatcher implementation.
#[derive(Debug, Clone)]
pub struct NatsSignalDispatcher {
    client: Arc<Client>,
    config: NatsSignalDispatcherConfig,
    subject_prefix: String,
}

impl NatsSignalDispatcher {
    pub async fn new(
        config: NatsSignalDispatcherConfig,
    ) -> Result<Self, SignalDispatcherError<Box<dyn std::error::Error + Send + Sync>>> {
        let prefix = config.subject_prefix.clone();
        match async_nats::connect(&config.nats_url).await {
            Ok(client) => Ok(Self {
                client: Arc::new(client),
                config,
                subject_prefix: prefix,
            }),
            Err(e) => Err(SignalDispatcherError::Subscribe(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            )),
        }
    }

    fn event_subject(&self, saga_id: &SagaId, event_type: &str) -> String {
        format!("{}.{}.{}", self.subject_prefix, saga_id.0, event_type)
    }

    fn pattern_subject(&self, pattern: &str) -> String {
        format!("{}.{}", self.subject_prefix, pattern)
    }
}

#[async_trait::async_trait]
impl SignalDispatcher for NatsSignalDispatcher {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn notify_new_event(
        &self,
        saga_id: &SagaId,
        event_id: u64,
    ) -> Result<(), SignalDispatcherError<Self::Error>> {
        let subject = self.event_subject(saga_id, "event");
        let payload = format!(
            r#"{{"saga_id":"{}","event_id":{},"timestamp":"{}"}}"#,
            saga_id.0,
            event_id,
            chrono::Utc::now().to_rfc3339()
        );
        if let Err(e) = self.client.publish(subject, payload.into()).await {
            return Err(SignalDispatcherError::Publish(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            ));
        }
        Ok(())
    }

    async fn notify_timer_fired(
        &self,
        saga_id: &SagaId,
        timer_id: &str,
    ) -> Result<(), SignalDispatcherError<Self::Error>> {
        let subject = self.event_subject(saga_id, "timer");
        let payload = format!(
            r#"{{"saga_id":"{}","timer_id":"{}","timestamp":"{}"}}"#,
            saga_id.0,
            timer_id,
            chrono::Utc::now().to_rfc3339()
        );
        if let Err(e) = self.client.publish(subject, payload.into()).await {
            return Err(SignalDispatcherError::Publish(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            ));
        }
        Ok(())
    }

    async fn notify_cancelled(
        &self,
        saga_id: &SagaId,
    ) -> Result<(), SignalDispatcherError<Self::Error>> {
        let subject = self.event_subject(saga_id, "cancelled");
        let payload = format!(
            r#"{{"saga_id":"{}","timestamp":"{}"}}"#,
            saga_id.0,
            chrono::Utc::now().to_rfc3339()
        );
        if let Err(e) = self.client.publish(subject, payload.into()).await {
            return Err(SignalDispatcherError::Publish(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            ));
        }
        Ok(())
    }

    async fn subscribe(
        &self,
        pattern: &str,
    ) -> Result<SignalStream, SignalDispatcherError<Self::Error>> {
        let subject = self.pattern_subject(pattern);
        let (tx, rx) = mpsc::channel(self.config.subscription_capacity);
        let mut subscription = match self.client.subscribe(subject).await {
            Ok(sub) => sub,
            Err(e) => {
                return Err(SignalDispatcherError::Subscribe(
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                ));
            }
        };
        let prefix = self.subject_prefix.clone();
        tokio::spawn(async move {
            while let Some(msg) = subscription.next().await {
                let notification = parse_signal_message(&msg.subject, &msg.payload, &prefix);
                let _ = tx.send(notification).await;
            }
        });
        Ok(SignalStream::new(rx))
    }

    async fn send_signal(
        &self,
        saga_id: &SagaId,
        signal_name: &str,
        payload: &[u8],
    ) -> Result<(), SignalDispatcherError<Self::Error>> {
        let subject = self.event_subject(saga_id, signal_name);
        if let Err(e) = self.client.publish(subject, payload.to_vec().into()).await {
            return Err(SignalDispatcherError::Publish(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            ));
        }
        Ok(())
    }
}

fn parse_signal_message(subject: &str, data: &[u8], prefix: &str) -> SignalNotification {
    let saga_prefix = format!("{}.", prefix);
    let remaining = subject.strip_prefix(&saga_prefix).unwrap_or(subject);
    let parts: Vec<&str> = remaining.split('.').collect();
    let saga_id = if !parts.is_empty() {
        SagaId(parts[0].to_string())
    } else {
        SagaId(subject.to_string())
    };
    let signal_type = if parts.len() > 1 {
        match parts[1] {
            "event" => SignalType::NewEvent,
            "timer" => SignalType::TimerFired,
            "cancelled" => SignalType::Cancelled,
            s => SignalType::External(s.to_string()),
        }
    } else {
        SignalType::External(subject.to_string())
    };
    SignalNotification {
        saga_id,
        signal_type,
        payload: data.to_vec(),
    }
}
