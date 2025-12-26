//! Global Log Buffer with Backpressure
//!
//! This module provides a global log buffer that enforces memory limits
//! and implements backpressure to prevent OOM errors in high-load scenarios.

pub mod backpressure;
pub mod global_buffer;
pub mod log_buffer;
pub mod metrics;

pub use backpressure::{BackpressureEvent, BackpressureStrategy};
pub use global_buffer::{GlobalLogBuffer, GlobalLogBufferError};
pub use log_buffer::{LogBuffer, LogEntry};
pub use metrics::LogBufferMetrics;

/// Module prelude
pub mod prelude {
    pub use super::{
        BackpressureEvent, BackpressureStrategy, GlobalLogBuffer, GlobalLogBufferError, LogBuffer,
        LogBufferMetrics, LogEntry,
    };
}
