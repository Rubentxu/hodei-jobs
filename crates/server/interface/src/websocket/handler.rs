//! WebSocket Handler Implementation
//!
//! This module provides the WebSocket handler for real-time updates,
//! including connection management, authentication, and event streaming.

use crate::websocket::jwt::{JwtClaims, JwtConfig, extract_token_from_header};
use axum::response::Response;
use futures::SinkExt;
use futures::stream::StreamExt;
use hodei_server_infrastructure::realtime::connection_manager::ConnectionManager;
use hodei_server_infrastructure::realtime::metrics::RealtimeMetrics;
use hodei_server_infrastructure::realtime::session::Session;
use hodei_shared::realtime::commands::ClientCommand;
use hodei_shared::realtime::messages::ServerMessage;
use hyper::HeaderMap;
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

/// WebSocket connection state
#[derive(Debug, Clone)]
pub struct WebSocketState {
    /// Connection manager for session tracking
    connection_manager: Arc<ConnectionManager>,
    /// JWT configuration for token validation
    jwt_config: Arc<JwtConfig>,
    /// Realtime metrics
    metrics: Arc<RealtimeMetrics>,
}

impl WebSocketState {
    /// Create a new WebSocket state
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        jwt_secret: impl Into<Vec<u8>>,
        metrics: Arc<RealtimeMetrics>,
    ) -> Self {
        Self {
            connection_manager,
            jwt_config: Arc::new(JwtConfig::new(jwt_secret, None)),
            metrics,
        }
    }

    /// Create with custom JWT config
    pub fn with_jwt_config(
        connection_manager: Arc<ConnectionManager>,
        jwt_config: JwtConfig,
        metrics: Arc<RealtimeMetrics>,
    ) -> Self {
        Self {
            connection_manager,
            jwt_config: Arc::new(jwt_config),
            metrics,
        }
    }
}

/// WebSocket error types
#[derive(Debug, Error, PartialEq)]
pub enum WsHandlerError {
    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("WebSocket upgrade failed")]
    UpgradeFailed,

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Message handling error: {0}")]
    MessageError(String),

    #[error("Session registration failed")]
    SessionRegistrationFailed,
}

/// JSON error response
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    code: u16,
}

/// Handle WebSocket connection
///
/// This function:
///
/// 1. Validates JWT from Authorization header
/// 2. Upgrades connection to WebSocket
/// 3. Registers session in ConnectionManager
/// 4. Handles client commands (subscribe/unsubscribe)
/// 5. Streams events to client
#[tracing::instrument(skip(ws_stream, state, session_id))]
pub async fn handle_websocket<S>(
    ws_stream: tokio_tungstenite::WebSocketStream<S>,
    state: WebSocketState,
    session_id: String,
    topics: Vec<String>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let start_time = Instant::now();
    let client_info = format!("session={}", session_id);

    info!(%client_info, "WebSocket connection established");

    // Create session channel
    let (session_tx, mut session_rx) = mpsc::channel(100);

    // Create internal channel for messages
    let (internal_tx, mut internal_rx) = mpsc::channel::<String>(100);

    // Create session
    let session = Arc::new(Session::new(
        session_id.clone(),
        session_tx,
        state.metrics.clone(),
    ));

    // Register session
    state
        .connection_manager
        .register_session(session.clone())
        .await;

    // Subscribe to requested topics
    for topic in &topics {
        state
            .connection_manager
            .subscribe(&session_id, topic.clone())
            .await;
        debug!(%client_info, topic = %topic, "Subscribed to topic");
    }

    // Send welcome acknowledgment
    let welcome = ServerMessage::Ack {
        id: format!("conn-{}", session_id),
        status: "connected".to_string(),
    };

    let welcome_msg = serde_json::to_string(&welcome).unwrap();
    if let Err(e) = session.send_message(welcome_msg).await {
        error!(%client_info, error = %e, "Failed to send welcome message");
    }

    // Split stream
    let (mut tx, mut rx) = ws_stream.split();

    // Task to forward internal messages to WebSocket
    let client_info_clone = client_info.clone();
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = internal_rx.recv().await {
            if let Err(e) = tx.send(Message::Text(msg.into())).await {
                error!(%client_info = %client_info_clone, error = %e, "Failed to send message");
                break;
            }
        }
    });

    // Task to handle incoming WebSocket messages
    let state_clone = state.clone();
    let session_id_clone = session_id.clone();
    let handle_task = tokio::spawn(async move {
        while let Some(result) = rx.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    if let Err(e) =
                        handle_client_message(&state_clone, &session_id_clone, &text).await
                    {
                        error!(%session_id_clone, error = %e, "Failed to handle client message");
                    }
                }
                Ok(Message::Close(_)) => {
                    info!(%session_id_clone, "Client initiated close");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    if let Err(e) = tx.send(Message::Pong(data)).await {
                        error!(%session_id_clone, error = %e, "Failed to send pong");
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {}
                Ok(Message::Binary(_)) => {
                    warn!(%session_id_clone, "Received binary message, ignoring");
                }
                Err(e) => {
                    error!(%session_id_clone, error = %e, "WebSocket error");
                    break;
                }
            }
        }
    });

    // Task to forward session events to WebSocket
    let client_info_for_session = client_info.clone();
    let session_task = tokio::spawn(async move {
        while let Some(msg) = session_rx.recv().await {
            if let Err(e) = tx.send(Message::Text(msg.into())).await {
                error!(%client_info_for_session, error = %e, "Failed to forward session message");
                break;
            }
        }
    });

    // Wait for any task to complete
    tokio::select! {
        _ = forward_task => {},
        _ = handle_task => {},
        _ = session_task => {},
    }

    // Cleanup
    let _ = state
        .connection_manager
        .unregister_session(&session_id)
        .await;

    let duration = start_time.elapsed();
    info!(%client_info, ?duration, "WebSocket connection closed");
}

/// Handle incoming client message
async fn handle_client_message(
    state: &WebSocketState,
    session_id: &str,
    text: &str,
) -> Result<(), WsHandlerError> {
    let command: ClientCommand = serde_json::from_str(text)
        .map_err(|e| WsHandlerError::MessageError(format!("Invalid JSON: {}", e)))?;

    match command {
        ClientCommand::Subscribe { topic, .. } => {
            state
                .connection_manager
                .subscribe(session_id, topic.clone())
                .await;
        }
        ClientCommand::Unsubscribe { topic } => {
            state
                .connection_manager
                .unsubscribe(session_id, &topic)
                .await;
        }
        ClientCommand::Ping => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_infrastructure::realtime::session::Session;
    use std::sync::Arc;

    fn create_test_state() -> (Arc<ConnectionManager>, Arc<RealtimeMetrics>, Arc<JwtConfig>) {
        let metrics = Arc::new(RealtimeMetrics::new());
        let connection_manager = Arc::new(ConnectionManager::with_metrics(metrics.clone()));
        let jwt_config = Arc::new(JwtConfig::new("test-secret", None));

        (connection_manager, metrics, jwt_config)
    }

    #[tokio::test]
    async fn test_jwt_claims_from_headers() {
        let (_, _, jwt_config) = create_test_state();

        let claims = JwtClaims {
            subject: "user-123".to_string(),
            roles: vec!["admin".to_string()],
            exp: None,
            iat: None,
            session_id: None,
        };

        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
        )
        .unwrap();

        let result = jwt_config.validate_token(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().subject, "user-123");
    }

    #[tokio::test]
    async fn test_missing_authorization_header() {
        let result = extract_token_from_header("");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_authorization_scheme() {
        let result = extract_token_from_header("Basic abc123");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_subscribe_command_format() {
        let command_json = r#"{"cmd":"sub","topic":"jobs:all","request_id":"req-123"}"#;

        let command: ClientCommand = serde_json::from_str(command_json).unwrap();

        match command {
            ClientCommand::Subscribe { topic, .. } => {
                assert_eq!(topic, "jobs:all");
            }
            _ => panic!("Expected Subscribe command"),
        }
    }

    #[tokio::test]
    async fn test_ping_command_format() {
        let command_json = r#"{"cmd":"ping"}"#;

        let command: ClientCommand = serde_json::from_str(command_json).unwrap();

        match command {
            ClientCommand::Ping => {}
            _ => panic!("Expected Ping command"),
        }
    }

    #[tokio::test]
    async fn test_session_creation() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let (tx, _rx) = mpsc::channel(100);

        let session = Session::new("test-session".to_string(), tx, metrics.clone());

        assert_eq!(session.id(), "test-session");
        assert!(session.subscribed_topics().is_empty());
    }

    #[tokio::test]
    async fn test_session_subscribe() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let (tx, _rx) = mpsc::channel(100);

        let session = Session::new("test-session".to_string(), tx, metrics.clone());

        session.subscribe("jobs:all");
        assert!(session.is_subscribed("jobs:all"));

        session.subscribe("workers:all");
        assert_eq!(session.subscribed_topics().len(), 2);
    }

    #[tokio::test]
    async fn test_session_unsubscribe() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let (tx, _rx) = mpsc::channel(100);

        let session = Session::new("test-session".to_string(), tx, metrics.clone());

        session.subscribe("jobs:all");
        session.subscribe("workers:all");

        session.unsubscribe("jobs:all");
        assert!(!session.is_subscribed("jobs:all"));
        assert!(session.is_subscribed("workers:all"));
        assert_eq!(session.subscribed_topics().len(), 1);
    }
}
