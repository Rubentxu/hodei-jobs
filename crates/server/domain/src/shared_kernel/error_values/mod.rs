//! Domain Error Value Objects
//!
//! Strongly-typed value objects that encapsulate validation rules for domain errors.
//! Eliminates Primitive Obsession by replacing raw types with validated Value Objects.

use crate::shared_kernel::DomainError;
use std::time::Duration;

/// Maximum number of retry attempts for a job.
///
/// Eliminates Primitive Obsession by encapsulating validation rules:
/// - Must be between 1 and 100 inclusive
/// - Provides type safety for retry logic
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaxAttempts(u32);

impl MaxAttempts {
    /// Minimum allowed value for max attempts
    pub const MIN: u32 = 1;
    /// Maximum allowed value for max attempts
    pub const MAX: u32 = 100;
    /// Default value for max attempts
    pub const DEFAULT: u32 = 3;

    /// Create a new MaxAttempts value object.
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value < Self::MIN {
            Err(DomainError::InvalidMaxAttempts {
                value,
                reason: format!("max_attempts must be at least {}", Self::MIN),
            })
        } else if value > Self::MAX {
            Err(DomainError::InvalidMaxAttempts {
                value,
                reason: format!("max_attempts cannot exceed {}", Self::MAX),
            })
        } else {
            Ok(Self(value))
        }
    }

    /// Create with a default value (3 attempts).
    pub fn default() -> Self {
        Self(Self::DEFAULT)
    }

    /// Get the underlying value.
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Check if this represents unlimited attempts.
    pub fn is_unlimited(&self) -> bool {
        self.0 == Self::MAX
    }

    /// Increment attempts by one.
    pub fn increment(&self) -> Self {
        Self(self.0.saturating_add(1).min(Self::MAX))
    }

    /// Check if we've exceeded the maximum attempts.
    pub fn has_exceeded(&self, current: u32) -> bool {
        current >= self.0
    }
}

impl std::fmt::Display for MaxAttempts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<MaxAttempts> for u32 {
    fn from(val: MaxAttempts) -> Self {
        val.value()
    }
}

impl TryFrom<u32> for MaxAttempts {
    type Error = DomainError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Job execution timeout value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobTimeout(Duration);

impl JobTimeout {
    /// Minimum timeout: 1 second
    pub const MIN_SECS: u64 = 1;
    /// Maximum timeout: 24 hours
    pub const MAX_SECS: u64 = 86_400;
    /// Default timeout: 5 minutes
    pub const DEFAULT_SECS: u64 = 300;

    /// Create a new JobTimeout from seconds.
    pub fn from_secs(seconds: u64) -> Result<Self, DomainError> {
        if seconds < Self::MIN_SECS {
            Err(DomainError::InvalidJobSpec {
                field: "timeout".to_string(),
                reason: format!("timeout must be at least {} second(s)", Self::MIN_SECS),
            })
        } else if seconds > Self::MAX_SECS {
            Err(DomainError::InvalidJobSpec {
                field: "timeout".to_string(),
                reason: format!(
                    "timeout cannot exceed {} seconds (24 hours)",
                    Self::MAX_SECS
                ),
            })
        } else {
            Ok(Self(Duration::from_secs(seconds)))
        }
    }

    /// Create with default timeout (5 minutes).
    pub fn default() -> Self {
        Self(Duration::from_secs(Self::DEFAULT_SECS))
    }

    /// Get timeout in seconds.
    pub fn as_secs(&self) -> u64 {
        self.0.as_secs()
    }

    /// Get timeout in minutes (rounded down).
    pub fn as_minutes(&self) -> u64 {
        self.0.as_secs() / 60
    }

    /// Get timeout in hours (rounded down).
    pub fn as_hours(&self) -> u64 {
        self.0.as_secs() / 3600
    }

    /// Check if this is a short timeout (less than 1 minute).
    pub fn is_short(&self) -> bool {
        self.0 < Duration::from_secs(60)
    }

    /// Check if this is a long timeout (more than 1 hour).
    pub fn is_long(&self) -> bool {
        self.0 > Duration::from_secs(3600)
    }
}

impl std::fmt::Display for JobTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let secs = self.0.as_secs();
        if secs >= 3600 {
            write!(f, "{}h", secs / 3600)
        } else if secs >= 60 {
            write!(f, "{}m", secs / 60)
        } else {
            write!(f, "{}s", secs)
        }
    }
}

impl From<JobTimeout> for Duration {
    fn from(val: JobTimeout) -> Self {
        val.0
    }
}

impl From<JobTimeout> for u64 {
    fn from(val: JobTimeout) -> Self {
        val.as_secs()
    }
}

impl TryFrom<u64> for JobTimeout {
    type Error = DomainError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::from_secs(value)
    }
}

/// Validation error for domain objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: String,
    pub reason: String,
    pub value: Option<String>,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
            value: None,
        }
    }

    pub fn with_value(
        field: impl Into<String>,
        reason: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
            value: Some(value.into()),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref value) = self.value {
            write!(f, "{}: {} (got: {})", self.field, self.reason, value)
        } else {
            write!(f, "{}: {}", self.field, self.reason)
        }
    }
}

impl std::error::Error for ValidationError {}

impl From<ValidationError> for DomainError {
    fn from(err: ValidationError) -> Self {
        DomainError::InvalidJobSpec {
            field: err.field,
            reason: err.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_attempts_new_valid() {
        let max = MaxAttempts::new(3).unwrap();
        assert_eq!(max.value(), 3);
    }

    #[test]
    fn test_max_attempts_new_min() {
        let max = MaxAttempts::new(1).unwrap();
        assert_eq!(max.value(), 1);
    }

    #[test]
    fn test_max_attempts_new_max() {
        let max = MaxAttempts::new(100).unwrap();
        assert_eq!(max.value(), 100);
    }

    #[test]
    fn test_max_attempts_new_below_min() {
        let result = MaxAttempts::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_max_attempts_new_above_max() {
        let result = MaxAttempts::new(101);
        assert!(result.is_err());
    }

    #[test]
    fn test_job_timeout_from_secs_valid() {
        let timeout = JobTimeout::from_secs(300).unwrap();
        assert_eq!(timeout.as_secs(), 300);
    }

    #[test]
    fn test_job_timeout_from_secs_below_min() {
        let result = JobTimeout::from_secs(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_job_timeout_from_secs_above_max() {
        let result = JobTimeout::from_secs(86401);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::new("field", "reason");
        assert_eq!(format!("{}", err), "field: reason");
    }
}
