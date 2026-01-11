//! Realtime WebSocket Protocol
//!
//! Provides types for real-time WebSocket communication between
//! server and clients (Leptos WASM frontend).

pub mod commands;
pub mod messages;

pub use commands::ClientCommand;
pub use messages::{ClientEvent, ClientEventPayload, ServerMessage};
