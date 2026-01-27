//! # Saga Engine Watchdog
//!
//! Hybrid watchdog system for saga engine with:
//! - Auto-polling health checks (30s configurable)
//! - Automatic recovery of failed components
//! - Stall detection (unprocessed events)
//! - Deadlock detection (stuck workflows)
//! - HTTP health endpoints for Kubernetes (on-demand)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │         SagaEngineWatchdog (Auto-polling)      │
//! │  - Polling interno (30s configurable)          │
//! │  - Auto-detection + Auto-recovery              │
//! └────────────────────┬────────────────────────────┘
//!                      │
//!                      ▼
//!          ┌──────────────────────┐
//!          │   HealthAggregator   │  ← Bridge entre ambos
//!          └────────────┬─────────┘
//!                       │
//!          ┌────────────┴────────────┐
//!          ▼                         ▼
//!    GET /health*              Componentes Internos
//!    (On-demand)              (Auto-managed)
//! ```
//!
//! ## Features
//!
//! - **Auto-polling**: Internal health checks every 30s (configurable)
//! - **Stall detection**: Detects unprocessed events/timers
//! - **Deadlock detection**: Detects stuck workflows
//! - **Auto-recovery**: Restarts failed components automatically
//! - **HTTP endpoints**: /health, /health/ready, /health/live for Kubernetes
//! - **Metrics**: Prometheus metrics for health monitoring
//!
//! ## Usage
//!
//! ```rust,no_run
//! use saga_engine_pg::watchdog::{SagaEngineWatchdog, HealthEndpoint};
//! use std::time::Duration;
//!
//! // Create watchdog
//! let watchdog = SagaEngineWatchdog::builder()
//!     .with_poll_interval(Duration::from_secs(30))
//!     .build();
//!
//! // Start watchdog
//! let shutdown_rx = watchdog.start().await?;
//!
//! // Create health endpoint
//! let health_endpoint = HealthEndpoint::new(watchdog.aggregator());
//!
//! // Register with HTTP server
//! // GET /health
//! // GET /health/ready
//! // GET /health/live
//! ```

mod health_check;
mod watchdog_component;
mod stall_detector;
mod deadlock_detector;
mod aggregator;
mod supervisor;
mod endpoint;
mod metrics;

pub use health_check::{HealthCheck, HealthStatus, HealthInfo, MetricValue};
pub use watchdog_component::{WatchdogComponent, RecoveryResult};
pub use stall_detector::{StallDetector, StallDetectorConfig, StallStatus, StallDetectorConfigBuilder};
pub use deadlock_detector::{DeadlockDetector, DeadlockDetectorConfig, DeadlockStatus, DeadlockDetectorConfigBuilder};
pub use aggregator::{HealthAggregator, OverallHealthStatus, ReadinessCheck};
pub use supervisor::{SagaEngineWatchdog, WatchdogConfig, WatchdogConfigBuilder};
pub use endpoint::{HealthEndpoint, HealthResponse, ReadinessResponse, LivenessResponse, ComponentHealth, ReadinessCheckResponse};
pub use metrics::{WatchdogMetrics, WatchdogAction, WatchdogActionMetrics};
