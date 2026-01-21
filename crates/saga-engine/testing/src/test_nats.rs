//! # Test NATS Helper
//!
//! Provides a test NATS server using testcontainers.
//! Automatically starts a container on startup.
//! Cleans up on drop.

use async_nats::{Client, ConnectionString};
use testcontainers::Container;
use testcontainers::Image;
use testcontainers::clients::DockerCli;
use testcontainers::core::WaitFor;

use crate::IntegrationError;

/// Configuration for test NATS server
#[derive(Debug, Clone)]
pub struct TestNatsConfig {
    pub image: String,
    pub tag: String,
    pub port: u16,
}

impl Default for TestNatsConfig {
    fn default() -> Self {
        Self {
            image: "nats".to_string(),
            tag: "2.10-alpine".to_string(),
            port: 4222,
        }
    }
}

/// NATS image for testcontainers
#[derive(Debug, Clone)]
pub struct NatsImage {
    image: String,
    tag: String,
}

impl Default for NatsImage {
    fn default() -> Self {
        Self {
            image: "nats".to_string(),
            tag: "2.10-alpine".to_string(),
        }
    }
}

impl Image for NatsImage {
    type Args = Vec<String>;

    fn name(&self) -> String {
        self.image.clone()
    }

    fn tag(&self) -> String {
        self.tag.clone()
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::text_on_stdout("Server is ready")]
    }
}

/// Test NATS wrapper that manages a NATS container
///
/// # Example
///
/// ```rust,ignore
/// #[tokio::test]
/// async fn test_with_nats() {
///     let nats = TestNats::new().await;
///     let client = nats.client().await.unwrap();
///     // Use client in tests...
/// }
/// ```
pub struct TestNats {
    _container: Container<'static, NatsImage>,
    connection_string: String,
    client: Option<Client>,
}

impl TestNats {
    /// Create a new test NATS server with default configuration
    pub async fn new() -> Result<Self, IntegrationError> {
        Self::with_config(TestNatsConfig::default()).await
    }

    /// Create a new test NATS server with custom configuration
    pub async fn with_config(config: TestNatsConfig) -> Result<Self, IntegrationError> {
        let docker = DockerCli::from_env()?;

        let image = NatsImage {
            image: config.image,
            tag: config.tag,
        };

        let container = docker.run(image);

        let host = container.get_host();
        let port = container.get_host_port(config.port)?;

        let connection_string = format!("nats://{}:{}", host, port);

        // Wait for NATS to be ready and connect
        let client = retry_connect(&connection_string).await?;

        Ok(Self {
            _container: container,
            connection_string,
            client: Some(client),
        })
    }

    /// Get the connection string
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }

    /// Get or create a NATS client
    pub async fn client(&mut self) -> Result<&mut Client, IntegrationError> {
        if self.client.is_none() {
            self.client = Some(retry_connect(&self.connection_string).await?);
        }
        Ok(self.client.as_mut().unwrap())
    }

    /// Get a new NATS client (creates a new connection)
    pub async fn new_client(&self) -> Result<Client, IntegrationError> {
        retry_connect(&self.connection_string).await
    }

    /// Publish a test message
    pub async fn publish(&self, subject: &str, payload: &[u8]) -> Result<(), async_nats::Error> {
        let mut client = self.new_client().await?;
        client.publish(subject.into(), payload.into()).await?;
        client.flush().await?;
        Ok(())
    }

    /// Subscribe to a test subject
    pub async fn subscribe(
        &self,
        subject: &str,
    ) -> Result<async_nats::Subscriber, async_nats::Error> {
        let client = self.new_client().await?;
        client.subscribe(subject.into()).await
    }
}

impl Drop for TestNats {
    fn drop(&mut self) {
        // Container is dropped automatically, which stops and removes the container
    }
}

/// Retry connecting to NATS with exponential backoff
async fn retry_connect(connection_string: &str) -> Result<Client, async_nats::Error> {
    let mut attempts = 0;
    let max_attempts = 10;

    loop {
        match async_nats::connect(connection_string).await {
            Ok(client) => return Ok(client),
            Err(e) if attempts < max_attempts => {
                attempts += 1;
                let delay = std::time::Duration::from_millis(500 * 2_u64.pow(attempts));
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Re-export the IntegrationError from test_database
pub use crate::test_database::IntegrationError;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nats_creation() {
        let nats = TestNats::new().await.unwrap();

        // Verify connection works
        let connection_string = nats.connection_string();
        assert!(connection_string.starts_with("nats://"));
    }

    #[tokio::test]
    async fn test_nats_publish_subscribe() {
        let nats = TestNats::new().await.unwrap();

        let subject = "test.subject";
        let payload = b"hello nats!";

        // Publish message
        nats.publish(subject, payload).await.unwrap();

        // Subscribe and receive
        let mut subscriber = nats.subscribe(subject).await.unwrap();

        // Use timeout for the receive
        let received = tokio::time::timeout(std::time::Duration::from_secs(5), subscriber.next())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(received.payload, payload);
    }
}
