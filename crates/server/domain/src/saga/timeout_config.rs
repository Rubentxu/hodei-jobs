//! Saga Timeout Configuration
//!
//! Provides configurable timeout values for different saga types.
//! This enables fine-tuned control over saga execution timeouts
//! based on the provider type (Kubernetes, Docker, etc.).
//!
//! # Usage:
///
/// ```rust
/// use hodei_server_domain::saga::{SagaTimeoutConfig, SagaType};
///
/// // Get timeout for provisioning saga on Kubernetes
/// let config = SagaTimeoutConfig::kubernetes();
/// let timeout = config.get_timeout(SagaType::Provisioning);
///
/// // Get timeout for execution saga on Docker
/// let config = SagaTimeoutConfig::docker_local();
/// let timeout = config.get_timeout(SagaType::Execution);
/// ```
use crate::saga::SagaType;
use std::time::Duration;

/// Configuration for saga timeouts based on saga type.
///
/// This struct provides configurable timeout values for each saga type,
/// allowing different timeout strategies for different infrastructure providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaTimeoutConfig {
    /// Timeout for provisioning sagas (worker creation)
    pub provisioning: Duration,
    /// Timeout for execution sagas (job dispatch and completion)
    pub execution: Duration,
    /// Timeout for recovery sagas (worker failure recovery)
    pub recovery: Duration,
    /// Timeout for cancellation sagas (job cancellation)
    pub cancellation: Duration,
    /// Timeout for timeout sagas (handling job timeouts)
    pub timeout: Duration,
    /// Timeout for cleanup sagas (orphaned resource cleanup)
    pub cleanup: Duration,
    /// Default timeout for unknown saga types
    pub default: Duration,
}

impl Default for SagaTimeoutConfig {
    fn default() -> Self {
        Self {
            provisioning: Duration::from_secs(300), // 5 minutes
            execution: Duration::from_secs(7200),   // 2 hours
            recovery: Duration::from_secs(900),     // 15 minutes
            cancellation: Duration::from_secs(120), // 2 minutes
            timeout: Duration::from_secs(120),      // 2 minutes
            cleanup: Duration::from_secs(300),      // 5 minutes
            default: Duration::from_secs(300),      // 5 minutes
        }
    }
}

impl SagaTimeoutConfig {
    /// Creates a configuration optimized for Kubernetes providers.
    ///
    /// Kubernetes provisioning typically takes longer due to pod scheduling,
    /// image pulling, and node allocation.
    #[inline]
    pub fn kubernetes() -> Self {
        Self {
            provisioning: Duration::from_secs(600), // 10 minutes for K8s pod creation
            execution: Duration::from_secs(7200),   // 2 hours
            recovery: Duration::from_secs(900),     // 15 minutes
            cancellation: Duration::from_secs(120), // 2 minutes
            timeout: Duration::from_secs(120),      // 2 minutes
            cleanup: Duration::from_secs(300),      // 5 minutes
            default: Duration::from_secs(300),      // 5 minutes
        }
    }

    /// Creates a configuration optimized for local Docker development.
    ///
    /// Docker provisioning is typically faster since containers start
    /// almost instantly on local machines.
    #[inline]
    pub fn docker_local() -> Self {
        Self {
            provisioning: Duration::from_secs(60), // 1 minute for local Docker
            execution: Duration::from_secs(3600),  // 1 hour
            recovery: Duration::from_secs(300),    // 5 minutes
            cancellation: Duration::from_secs(30), // 30 seconds
            timeout: Duration::from_secs(60),      // 1 minute
            cleanup: Duration::from_secs(60),      // 1 minute
            default: Duration::from_secs(60),      // 1 minute
        }
    }

    /// Creates a configuration optimized for Firecracker microVMs.
    ///
    /// Firecracker VMs have faster startup than full VMs but slower than containers.
    #[inline]
    pub fn firecracker() -> Self {
        Self {
            provisioning: Duration::from_secs(120), // 2 minutes for microVM
            execution: Duration::from_secs(7200),   // 2 hours
            recovery: Duration::from_secs(600),     // 10 minutes
            cancellation: Duration::from_secs(60),  // 1 minute
            timeout: Duration::from_secs(90),       // 90 seconds
            cleanup: Duration::from_secs(120),      // 2 minutes
            default: Duration::from_secs(120),      // 2 minutes
        }
    }

    /// Gets the timeout for a specific saga type.
    ///
    /// # Arguments
    ///
    /// * `saga_type` - The type of saga to get the timeout for
    ///
    /// # Returns
    ///
    /// The configured timeout duration for the saga type,
    /// or the default timeout if the type is unknown.
    #[inline]
    pub fn get_timeout(&self, saga_type: SagaType) -> Duration {
        match saga_type {
            SagaType::Provisioning => self.provisioning,
            SagaType::Execution => self.execution,
            SagaType::Recovery => self.recovery,
            SagaType::Cancellation => self.cancellation,
            SagaType::Timeout => self.timeout,
            SagaType::Cleanup => self.cleanup,
        }
    }

    /// Creates a custom configuration with all timeouts specified.
    ///
    /// # Arguments
    ///
    /// * `provisioning` - Timeout for provisioning sagas
    /// * `execution` - Timeout for execution sagas
    /// * `recovery` - Timeout for recovery sagas
    /// * `cancellation` - Timeout for cancellation sagas
    /// * `timeout` - Timeout for timeout sagas
    /// * `cleanup` - Timeout for cleanup sagas
    ///
    /// # Returns
    ///
    /// A new `SagaTimeoutConfig` with the specified timeouts.
    #[inline]
    pub fn new(
        provisioning: Duration,
        execution: Duration,
        recovery: Duration,
        cancellation: Duration,
        timeout: Duration,
        cleanup: Duration,
    ) -> Self {
        Self {
            provisioning,
            execution,
            recovery,
            cancellation,
            timeout,
            cleanup,
            default: provisioning, // Default to provisioning timeout
        }
    }

    /// Updates a single timeout value.
    ///
    /// # Arguments
    ///
    /// * `saga_type` - The saga type to update
    /// * `timeout` - The new timeout duration
    ///
    /// # Returns
    ///
    /// A new `SagaTimeoutConfig` with the updated timeout.
    #[inline]
    pub fn with_timeout(mut self, saga_type: SagaType, timeout: Duration) -> Self {
        match saga_type {
            SagaType::Provisioning => self.provisioning = timeout,
            SagaType::Execution => self.execution = timeout,
            SagaType::Recovery => self.recovery = timeout,
            SagaType::Cancellation => self.cancellation = timeout,
            SagaType::Timeout => self.timeout = timeout,
            SagaType::Cleanup => self.cleanup = timeout,
        }
        self
    }
}

/// Trait for types that can provide saga timeouts.
///
/// This trait enables dependency injection of timeout configurations,
/// making sagas more testable and configurable.
pub trait TimeoutAware {
    /// Gets the timeout configuration.
    fn timeout_config(&self) -> &SagaTimeoutConfig;

    /// Gets the timeout for a specific saga type.
    #[inline]
    fn timeout_for(&self, saga_type: SagaType) -> Duration {
        self.timeout_config().get_timeout(saga_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::SagaType;

    #[test]
    fn default_config_has_expected_values() {
        let config = SagaTimeoutConfig::default();

        assert_eq!(config.provisioning, Duration::from_secs(300));
        assert_eq!(config.execution, Duration::from_secs(7200));
        assert_eq!(config.recovery, Duration::from_secs(900));
        assert_eq!(config.cancellation, Duration::from_secs(120));
        assert_eq!(config.timeout, Duration::from_secs(120));
        assert_eq!(config.cleanup, Duration::from_secs(300));
    }

    #[test]
    fn kubernetes_config_has_longer_provisioning_timeout() {
        let config = SagaTimeoutConfig::kubernetes();

        // Kubernetes provisioning should be longer than default
        assert!(config.provisioning > Duration::from_secs(300));
        assert_eq!(config.provisioning, Duration::from_secs(600));
    }

    #[test]
    fn docker_local_config_has_shorter_timeouts() {
        let config = SagaTimeoutConfig::docker_local();

        // Docker local should have shorter timeouts
        assert_eq!(config.provisioning, Duration::from_secs(60));
        assert_eq!(config.cancellation, Duration::from_secs(30));
    }

    #[test]
    fn get_timeout_returns_correct_values() {
        let config = SagaTimeoutConfig::default();

        assert_eq!(
            config.get_timeout(SagaType::Provisioning),
            Duration::from_secs(300)
        );
        assert_eq!(
            config.get_timeout(SagaType::Execution),
            Duration::from_secs(7200)
        );
        assert_eq!(
            config.get_timeout(SagaType::Recovery),
            Duration::from_secs(900)
        );
        assert_eq!(
            config.get_timeout(SagaType::Cancellation),
            Duration::from_secs(120)
        );
        assert_eq!(
            config.get_timeout(SagaType::Timeout),
            Duration::from_secs(120)
        );
        assert_eq!(
            config.get_timeout(SagaType::Cleanup),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn with_timeout_updates_correct_value() {
        let config = SagaTimeoutConfig::default()
            .with_timeout(SagaType::Execution, Duration::from_secs(3600));

        assert_eq!(
            config.get_timeout(SagaType::Execution),
            Duration::from_secs(3600)
        );
        // Other values unchanged
        assert_eq!(
            config.get_timeout(SagaType::Provisioning),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn new_creates_config_with_specified_values() {
        let config = SagaTimeoutConfig::new(
            Duration::from_secs(100),
            Duration::from_secs(200),
            Duration::from_secs(300),
            Duration::from_secs(40),
            Duration::from_secs(50),
            Duration::from_secs(60),
        );

        assert_eq!(config.provisioning, Duration::from_secs(100));
        assert_eq!(config.execution, Duration::from_secs(200));
        assert_eq!(config.recovery, Duration::from_secs(300));
        assert_eq!(config.cancellation, Duration::from_secs(40));
        assert_eq!(config.timeout, Duration::from_secs(50));
        assert_eq!(config.cleanup, Duration::from_secs(60));
    }

    #[test]
    fn clone_works_correctly() {
        let original = SagaTimeoutConfig::kubernetes();
        let cloned = original.clone();

        assert_eq!(original, cloned);
    }
}
