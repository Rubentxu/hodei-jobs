//! WebSocket Handler Module for Real-time Updates
//!
//! This module provides WebSocket connectivity for the Hodei Jobs platform,
//! enabling real-time updates for jobs, workers, and system events.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         Axum Router                              │
//! │                         /api/v1/ws                               │
//└─────────────────────────────┬─────────────────────────────────────┘
//                              │
//                              ▼
//┌─────────────────────────────────────────────────────────────────┐
//│                      WebSocket Handler                           │
//│  ┌──────────────────────────────────────────────────────────┐    │
//│  │  1. Extract Authorization header                          │    │
//│  │  2. Validate JWT token                                    │    │
//│  │  3. Extract connection and upgrade to WebSocket           │    │
//│  │  4. Register session in ConnectionManager                 │    │
//│  │  5. Handle client commands (subscribe/unsubscribe)        │    │
//│  │  6. Stream events to client                               │    │
//│  └──────────────────────────────────────────────────────────┘    │
//└─────────────────────────────┬─────────────────────────────────────┘
//                              │
//            ┌─────────────────┼─────────────────┐
//            ▼                 ▼                 ▼
//   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
//   │ Job Events   │  │ Worker Events│  │System Events │
//   │ jobs:all     │  │workers:all   │  │  agg:{id}    │
//   │ agg:{job_id} │  │agg:{worker}  │  └──────────────┘
//   └──────────────┘  └──────────────┘
//!
//! ## Security
//!
//! - JWT tokens must be provided in the `Authorization` header
//! - Bearer token format: `Authorization: Bearer <token>`
//! - Token validation includes expiration check
//!
//! ## Usage
//!
//! ```rust
//! use hodei_server_interface::websocket::{WebSocketState, ws_handler};
//!
//! let state = WebSocketState::new(connection_manager, jwt_secret);
//! let router = Router::new().route("/api/v1/ws", get(ws_handler));
//! ```

mod handler;
mod jwt;

pub use handler::{WebSocketState, WsHandlerError};
pub use jwt::{JwtClaims, JwtConfig, JwtError, extract_token_from_header};

// Re-export types from shared crate
pub use hodei_shared::realtime::commands::ClientCommand;
pub use hodei_shared::realtime::messages::ServerMessage;
