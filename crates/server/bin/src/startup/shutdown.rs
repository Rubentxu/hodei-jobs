//! Graceful Shutdown Module
//!
//! Implements coordinated shutdown for all server components with:
//! - Signal handlers (SIGTERM, SIGINT)
//! - Graceful draining of in-progress messages
//! - Configurable shutdown timeout
//! - Coordination between all background tasks

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::{broadcast, oneshot, watch};
use tokio::time::timeout;
use tracing::{error, info, warn};

/// Shutdown configuration
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Maximum time to wait for graceful shutdown
    pub timeout: Duration,
    /// Time to wait before force shutdown
    pub force_delay: Duration,
    /// Enable signal handlers
    pub enable_signals: bool,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            force_delay: Duration::from_secs(5),
            enable_signals: true,
        }
    }
}

impl ShutdownConfig {
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_force_delay(mut self, delay: Duration) -> Self {
        self.force_delay = delay;
        self
    }
}

/// Graceful shutdown coordinator
///
/// Coordinates shutdown across multiple components by:
/// 1. Receiving shutdown signal (SIGTERM/SIGINT or programmatic)
/// 2. Notifying all registered components to stop accepting new work
/// 3. Waiting for in-progress work to complete (or timeout)
/// 4. Forcing shutdown if timeout exceeded
#[derive(Clone)]
pub struct GracefulShutdown {
    /// Broadcast channel for shutdown signal
    shutdown_tx: Arc<broadcast::Sender<ShutdownSignal>>,
    /// Watch channel for shutdown state (sender and receiver for consistent state access)
    state_tx: Arc<watch::Sender<ShutdownState>>,
    /// Receiver for shutdown state (stored for consistent state access)
    state_rx: Arc<watch::Receiver<ShutdownState>>,
    /// Shutdown config
    config: Arc<ShutdownConfig>,
}

impl GracefulShutdown {
    /// Create a new GracefulShutdown coordinator
    pub fn new(config: ShutdownConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(16);
        let (state_tx, state_rx) = watch::channel(ShutdownState::Running);

        Self {
            shutdown_tx: Arc::new(shutdown_tx),
            state_tx: Arc::new(state_tx),
            state_rx: Arc::new(state_rx),
            config: Arc::new(config),
        }
    }

    /// Get a handle for subscribing to shutdown signals
    pub fn subscribe(&self) -> ShutdownReceiver {
        ShutdownReceiver {
            rx: Arc::new(tokio::sync::Mutex::new(self.shutdown_tx.subscribe())),
            state_rx: self.state_tx.subscribe(),
        }
    }

    /// Trigger shutdown programmatically
    pub fn shutdown(&self, reason: ShutdownReason) {
        info!("Triggering shutdown: {:?}", reason);
        let _ = self
            .state_tx
            .send(ShutdownState::ShuttingDown(reason.clone()));
        let _ = self.shutdown_tx.send(ShutdownSignal {
            reason,
            timestamp: chrono::Utc::now(),
        });
    }

    /// Wait for shutdown signal (from signals or programmatic)
    pub async fn wait_for_signal(&self) -> ShutdownSignal {
        let mut rx = self.subscribe();
        rx.recv().await
    }

    /// Run with graceful shutdown support
    pub async fn run_with_shutdown<F, R, E>(&self, task: F, task_name: &str) -> Result<R, E>
    where
        F: Future<Output = Result<R, E>> + Send + 'static,
        R: Send + 'static,
        E: std::fmt::Display + From<String> + Send + 'static,
    {
        // Spawn the main task
        let task_handle = tokio::spawn(task);

        // Wait for either task completion or shutdown signal
        tokio::select! {
            result = task_handle => {
                result.unwrap_or_else(|e| {
                    error!("Task {} panicked: {}", task_name, e);
                    panic!("Task {} panic: {}", task_name, e)
                })
            }
            signal = self.wait_for_signal() => {
                info!("Shutdown signal received during {}: {:?}", task_name, signal);
                // Signal the task to stop (it should respect the shutdown receiver)
                Err(format!("Shutdown during {}: {:?}", task_name, signal).into())
            }
        }
    }

    /// Get current state
    pub fn state(&self) -> ShutdownState {
        (*self.state_rx.borrow()).clone()
    }

    /// Check if shutdown has been initiated
    pub fn is_shutting_down(&self) -> bool {
        matches!(*self.state_rx.borrow(), ShutdownState::ShuttingDown(_))
    }
}

/// Receiver for shutdown signals
#[derive(Clone)]
pub struct ShutdownReceiver {
    rx: Arc<tokio::sync::Mutex<broadcast::Receiver<ShutdownSignal>>>,
    state_rx: watch::Receiver<ShutdownState>,
}

impl ShutdownReceiver {
    /// Receive the next shutdown signal
    pub async fn recv(&mut self) -> ShutdownSignal {
        let mut rx = self.rx.lock().await;
        rx.recv().await.unwrap_or_else(|_e| {
            // If sender dropped, use the latest state
            match &*self.state_rx.borrow() {
                ShutdownState::Running => ShutdownSignal {
                    reason: ShutdownReason::Unknown,
                    timestamp: chrono::Utc::now(),
                },
                ShutdownState::ShuttingDown(reason) => ShutdownSignal {
                    reason: reason.clone(),
                    timestamp: chrono::Utc::now(),
                },
                ShutdownState::Completed => ShutdownSignal {
                    reason: ShutdownReason::Unknown,
                    timestamp: chrono::Utc::now(),
                },
            }
        })
    }

    /// Get current state
    pub fn state(&self) -> ShutdownState {
        self.state_rx.borrow().clone()
    }

    /// Check if shutdown has been initiated
    pub fn is_shutting_down(&self) -> bool {
        matches!(&*self.state_rx.borrow(), ShutdownState::ShuttingDown(_))
    }
}

/// Shutdown signal information
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    pub reason: ShutdownReason,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl std::fmt::Display for ShutdownSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.reason, self.timestamp)
    }
}

/// Reason for shutdown
#[derive(Debug, Clone, PartialEq)]
pub enum ShutdownReason {
    /// SIGTERM signal received
    SigTerm,
    /// SIGINT signal received (Ctrl+C)
    SigInt,
    /// programmatic shutdown
    Programmatic(String),
    /// Health check failed
    HealthCheckFailed(String),
    /// Unknown reason
    Unknown,
}

impl std::fmt::Display for ShutdownReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShutdownReason::SigTerm => write!(f, "SIGTERM"),
            ShutdownReason::SigInt => write!(f, "SIGINT (Ctrl+C)"),
            ShutdownReason::Programmatic(reason) => write!(f, "Programmatic: {}", reason),
            ShutdownReason::HealthCheckFailed(reason) => {
                write!(f, "Health check failed: {}", reason)
            }
            ShutdownReason::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Shutdown state
#[derive(Debug, Clone, PartialEq)]
pub enum ShutdownState {
    /// Normal running state
    Running,
    /// Shutdown initiated
    ShuttingDown(ShutdownReason),
    /// Shutdown completed
    Completed,
}

/// Component handle for graceful shutdown
pub struct ShutdownHandle {
    /// Sender signal component to stop
    stop_tx: Option<oneshot::Sender<()>>,
    /// Join handle for the component task
    _join_handle: tokio::task::JoinHandle<()>,
}

impl ShutdownHandle {
    /// Create a new shutdown handle
    pub fn new<F>(_stop_rx: oneshot::Receiver<()>, f: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let (stop_tx, stop_rx_inner) = oneshot::channel();
        let join_handle = tokio::spawn(async move {
            tokio::select! {
                _ = stop_rx_inner => {
                    info!("Component received stop signal");
                }
                _ = f => {
                    info!("Component task completed");
                }
            }
        });

        Self {
            stop_tx: Some(stop_tx),
            _join_handle: join_handle,
        }
    }

    /// Signal the component to stop
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Trait for components that support graceful shutdown
#[async_trait::async_trait]
pub trait Shutdownable: Send {
    /// Signal the component to stop accepting new work
    fn prepare_shutdown(&mut self);
    /// Wait for the component to finish in-progress work
    async fn wait_for_shutdown(&mut self, timeout: Duration) -> bool;
}

/// Execute shutdown sequence for multiple components
pub async fn execute_shutdown_sequence(
    _coordinator: &GracefulShutdown,
    mut components: Vec<&mut dyn Shutdownable>,
    config: &ShutdownConfig,
) -> bool {
    info!(
        "Starting graceful shutdown sequence with {} components",
        components.len()
    );

    // Phase 1: Signal all components to stop accepting new work
    for (i, component) in components.iter_mut().enumerate() {
        info!("Preparing component {} for shutdown", i);
        component.prepare_shutdown();
    }

    // Phase 2: Wait for in-progress work to complete
    let shutdown_futures: Vec<_> = components
        .iter_mut()
        .enumerate()
        .map(|(i, component)| async move {
            let result = component.wait_for_shutdown(config.timeout).await;
            info!(
                "Component {} shutdown: {}",
                i,
                if result { "OK" } else { "TIMEOUT" }
            );
            result
        })
        .collect();

    // Wait for all components with overall timeout
    let results = timeout(config.timeout, async {
        let mut success = true;
        for future in shutdown_futures {
            if !future.await {
                success = false;
            }
        }
        success
    })
    .await;

    match results {
        Ok(true) => {
            info!("All components shut down gracefully");
            true
        }
        Ok(false) => {
            warn!("Some components did not shut down in time");
            false
        }
        Err(_) => {
            warn!("Shutdown sequence timed out after {:?}", config.timeout);
            false
        }
    }
}

/// Start signal handler that triggers graceful shutdown
pub async fn start_signal_handler(coordinator: &GracefulShutdown) {
    if !coordinator.config.enable_signals {
        return;
    }

    // Create a clone for the spawned task
    let coordinator = coordinator.clone();

    tokio::spawn(async move {
        // Wait for either SIGTERM or SIGINT
        let ctrl_c = async {
            match signal::ctrl_c().await {
                Ok(()) => ShutdownReason::SigInt,
                Err(e) => {
                    tracing::error!("Failed to register ctrl-c handler: {}", e);
                    ShutdownReason::Unknown
                }
            }
        };

        let term = async {
            // Register for SIGTERM
            match signal::unix::signal(signal::unix::SignalKind::terminate()) {
                Ok(mut sig) => {
                    sig.recv().await;
                    ShutdownReason::SigTerm
                }
                Err(e) => {
                    tracing::error!("Failed to register SIGTERM handler: {}", e);
                    ShutdownReason::Unknown
                }
            }
        };

        tokio::select! {
            reason = ctrl_c => {
                coordinator.shutdown(reason);
            }
            reason = term => {
                coordinator.shutdown(reason);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_graceful_shutdown_trigger() {
        let shutdown = GracefulShutdown::new(ShutdownConfig::default());

        // Trigger shutdown programmatically
        let handle = tokio::spawn({
            let shutdown = shutdown.clone();
            async move { shutdown.wait_for_signal().await }
        });

        // Give it a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Trigger shutdown
        shutdown.shutdown(ShutdownReason::Programmatic("Test".to_string()));

        let signal = handle.await.unwrap();
        assert!(matches!(signal.reason, ShutdownReason::Programmatic(_)));
    }

    #[tokio::test]
    async fn test_shutdown_state() {
        let shutdown = GracefulShutdown::new(ShutdownConfig::default());

        assert!(!shutdown.is_shutting_down());
        assert!(matches!(shutdown.state(), ShutdownState::Running));

        shutdown.shutdown(ShutdownReason::SigInt);

        assert!(shutdown.is_shutting_down());
        assert!(matches!(shutdown.state(), ShutdownState::ShuttingDown(_)));
    }

    #[tokio::test]
    async fn test_shutdown_receiver() {
        let shutdown = GracefulShutdown::new(ShutdownConfig::default());
        let mut receiver = shutdown.subscribe();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            shutdown.shutdown(ShutdownReason::SigTerm);
        });

        let signal = receiver.recv().await;
        assert!(matches!(signal.reason, ShutdownReason::SigTerm));
    }
}
