use serde::{Deserialize, Serialize};
use std::fmt;

/// Estados posibles de un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobState {
    Pending,
    Scheduled,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Timeout,
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobState::Pending => write!(f, "PENDING"),
            JobState::Scheduled => write!(f, "SCHEDULED"),
            JobState::Running => write!(f, "RUNNING"),
            JobState::Succeeded => write!(f, "SUCCEEDED"),
            JobState::Failed => write!(f, "FAILED"),
            JobState::Cancelled => write!(f, "CANCELLED"),
            JobState::Timeout => write!(f, "TIMEOUT"),
        }
    }
}

/// Estados del ciclo de vida de un worker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkerState {
    Creating,
    Connecting,
    Ready,
    Busy,
    Draining,
    Terminating,
    Terminated,
}

impl fmt::Display for WorkerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerState::Creating => write!(f, "CREATING"),
            WorkerState::Connecting => write!(f, "CONNECTING"),
            WorkerState::Ready => write!(f, "READY"),
            WorkerState::Busy => write!(f, "BUSY"),
            WorkerState::Draining => write!(f, "DRAINING"),
            WorkerState::Terminating => write!(f, "TERMINATING"),
            WorkerState::Terminated => write!(f, "TERMINATED"),
        }
    }
}

impl WorkerState {
    pub fn can_accept_jobs(&self) -> bool {
        matches!(self, WorkerState::Ready)
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            WorkerState::Creating
                | WorkerState::Connecting
                | WorkerState::Ready
                | WorkerState::Busy
                | WorkerState::Draining
        )
    }

    pub fn is_terminated(&self) -> bool {
        matches!(self, WorkerState::Terminated)
    }

    pub fn is_draining(&self) -> bool {
        matches!(self, WorkerState::Draining)
    }
}

/// Estados posibles de un provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderStatus {
    Active,
    Maintenance,
    Disabled,
    Overloaded,
    Unhealthy,
    Degraded,
}

impl fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderStatus::Active => write!(f, "ACTIVE"),
            ProviderStatus::Maintenance => write!(f, "MAINTENANCE"),
            ProviderStatus::Disabled => write!(f, "DISABLED"),
            ProviderStatus::Overloaded => write!(f, "OVERLOADED"),
            ProviderStatus::Unhealthy => write!(f, "UNHEALTHY"),
            ProviderStatus::Degraded => write!(f, "DEGRADED"),
        }
    }
}

/// Estados de ejecución específicos del provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Submitted,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionStatus::Submitted => write!(f, "SUBMITTED"),
            ExecutionStatus::Queued => write!(f, "QUEUED"),
            ExecutionStatus::Running => write!(f, "RUNNING"),
            ExecutionStatus::Succeeded => write!(f, "SUCCEEDED"),
            ExecutionStatus::Failed => write!(f, "FAILED"),
            ExecutionStatus::Cancelled => write!(f, "CANCELLED"),
            ExecutionStatus::TimedOut => write!(f, "TIMED_OUT"),
        }
    }
}

/// Resultado de ejecución de un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobResult {
    Success {
        exit_code: i32,
        output: String,
        error_output: String,
    },
    Failed {
        exit_code: i32,
        error_message: String,
        error_output: String,
    },
    Cancelled,
    Timeout,
}

impl fmt::Display for JobResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobResult::Success { exit_code, .. } => {
                write!(f, "SUCCESS (exit_code: {})", exit_code)
            }
            JobResult::Failed {
                exit_code,
                error_message,
                ..
            } => {
                write!(f, "FAILED (exit_code: {}): {}", exit_code, error_message)
            }
            JobResult::Cancelled => write!(f, "CANCELLED"),
            JobResult::Timeout => write!(f, "TIMEOUT"),
        }
    }
}
