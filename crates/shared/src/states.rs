use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Estados posibles de un job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobState {
    Pending,
    Assigned,
    Scheduled,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Timeout,
}

impl JobState {
    /// Valida si una transición de estado es válida según el State Machine del dominio
    ///
    /// Transiciones válidas:
    /// - Pending → Assigned, Scheduled, Failed, Cancelled, Timeout
    /// - Assigned → Running, Failed, Timeout, Cancelled
    /// - Scheduled → Running, Failed, Timeout, Cancelled
    /// - Running → Succeeded, Failed, Cancelled, Timeout
    /// - Succeeded, Failed, Cancelled, Timeout → (terminal, no transiciones salientes)
    pub fn can_transition_to(&self, new_state: &JobState) -> bool {
        match (self, new_state) {
            // Mismo estado - no es una transición válida
            (s, n) if s == n => false,

            // Transiciones válidas desde Pending
            (JobState::Pending, JobState::Assigned) => true,
            (JobState::Pending, JobState::Scheduled) => true,
            (JobState::Pending, JobState::Failed) => true,
            (JobState::Pending, JobState::Cancelled) => true,
            (JobState::Pending, JobState::Timeout) => true,

            // Transiciones válidas desde Assigned
            (JobState::Assigned, JobState::Running) => true,
            (JobState::Assigned, JobState::Failed) => true,
            (JobState::Assigned, JobState::Timeout) => true,
            (JobState::Assigned, JobState::Cancelled) => true,

            // Transiciones válidas desde Scheduled
            (JobState::Scheduled, JobState::Running) => true,
            (JobState::Scheduled, JobState::Failed) => true,
            (JobState::Scheduled, JobState::Timeout) => true,
            (JobState::Scheduled, JobState::Cancelled) => true,

            // Transiciones válidas desde Running
            (JobState::Running, JobState::Succeeded) => true,
            (JobState::Running, JobState::Failed) => true,
            (JobState::Running, JobState::Cancelled) => true,
            (JobState::Running, JobState::Timeout) => true,

            // Todas las demás transiciones son inválidas
            // Esto incluye:
            // - Estados terminales que intentan transicionar
            // - Transiciones hacia Pending (no se puede volver atrás)
            // - Transiciones hacia Assigned desde cualquier estado excepto Pending
            // - Transiciones hacia Scheduled desde cualquier estado excepto Pending
            _ => false,
        }
    }

    /// Retorna true si el estado es terminal (no se puede continuar)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobState::Succeeded | JobState::Failed | JobState::Cancelled | JobState::Timeout
        )
    }

    /// Retorna true si el estado está en progreso
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            JobState::Pending | JobState::Assigned | JobState::Scheduled | JobState::Running
        )
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobState::Pending => write!(f, "PENDING"),
            JobState::Assigned => write!(f, "ASSIGNED"),
            JobState::Scheduled => write!(f, "SCHEDULED"),
            JobState::Running => write!(f, "RUNNING"),
            JobState::Succeeded => write!(f, "SUCCEEDED"),
            JobState::Failed => write!(f, "FAILED"),
            JobState::Cancelled => write!(f, "CANCELLED"),
            JobState::Timeout => write!(f, "TIMEOUT"),
        }
    }
}

impl FromStr for JobState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PENDING" => Ok(JobState::Pending),
            "ASSIGNED" => Ok(JobState::Assigned),
            "SCHEDULED" => Ok(JobState::Scheduled),
            "RUNNING" => Ok(JobState::Running),
            "SUCCEEDED" => Ok(JobState::Succeeded),
            "FAILED" => Ok(JobState::Failed),
            "CANCELLED" => Ok(JobState::Cancelled),
            "TIMEOUT" => Ok(JobState::Timeout),
            _ => Err(format!("Invalid JobState: {}", s)),
        }
    }
}

impl TryFrom<i32> for JobState {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(JobState::Pending),
            1 => Ok(JobState::Assigned),
            2 => Ok(JobState::Scheduled),
            3 => Ok(JobState::Running),
            4 => Ok(JobState::Succeeded),
            5 => Ok(JobState::Failed),
            6 => Ok(JobState::Cancelled),
            7 => Ok(JobState::Timeout),
            _ => Err(format!("Invalid JobState value: {}", value)),
        }
    }
}

impl From<&JobState> for i32 {
    fn from(state: &JobState) -> Self {
        match state {
            JobState::Pending => 0,
            JobState::Assigned => 1,
            JobState::Scheduled => 2,
            JobState::Running => 3,
            JobState::Succeeded => 4,
            JobState::Failed => 5,
            JobState::Cancelled => 6,
            JobState::Timeout => 7,
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

impl FromStr for WorkerState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CREATING" => Ok(WorkerState::Creating),
            "CONNECTING" => Ok(WorkerState::Connecting),
            "READY" => Ok(WorkerState::Ready),
            "BUSY" => Ok(WorkerState::Busy),
            "DRAINING" => Ok(WorkerState::Draining),
            "TERMINATING" => Ok(WorkerState::Terminating),
            "TERMINATED" => Ok(WorkerState::Terminated),
            _ => Err(format!("Invalid WorkerState: {}", s)),
        }
    }
}

impl TryFrom<i32> for WorkerState {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WorkerState::Creating),
            1 => Ok(WorkerState::Connecting),
            2 => Ok(WorkerState::Ready),
            3 => Ok(WorkerState::Busy),
            4 => Ok(WorkerState::Draining),
            5 => Ok(WorkerState::Terminating),
            6 => Ok(WorkerState::Terminated),
            _ => Err(format!("Invalid WorkerState value: {}", value)),
        }
    }
}

impl From<&WorkerState> for i32 {
    fn from(state: &WorkerState) -> Self {
        match state {
            WorkerState::Creating => 0,
            WorkerState::Connecting => 1,
            WorkerState::Ready => 2,
            WorkerState::Busy => 3,
            WorkerState::Draining => 4,
            WorkerState::Terminating => 5,
            WorkerState::Terminated => 6,
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

impl FromStr for ProviderStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ACTIVE" => Ok(ProviderStatus::Active),
            "MAINTENANCE" => Ok(ProviderStatus::Maintenance),
            "DISABLED" => Ok(ProviderStatus::Disabled),
            "OVERLOADED" => Ok(ProviderStatus::Overloaded),
            "UNHEALTHY" => Ok(ProviderStatus::Unhealthy),
            "DEGRADED" => Ok(ProviderStatus::Degraded),
            _ => Err(format!("Invalid ProviderStatus: {}", s)),
        }
    }
}

impl TryFrom<i32> for ProviderStatus {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProviderStatus::Active),
            1 => Ok(ProviderStatus::Maintenance),
            2 => Ok(ProviderStatus::Disabled),
            3 => Ok(ProviderStatus::Overloaded),
            4 => Ok(ProviderStatus::Unhealthy),
            5 => Ok(ProviderStatus::Degraded),
            _ => Err(format!("Invalid ProviderStatus value: {}", value)),
        }
    }
}

impl From<&ProviderStatus> for i32 {
    fn from(status: &ProviderStatus) -> Self {
        match status {
            ProviderStatus::Active => 0,
            ProviderStatus::Maintenance => 1,
            ProviderStatus::Disabled => 2,
            ProviderStatus::Overloaded => 3,
            ProviderStatus::Unhealthy => 4,
            ProviderStatus::Degraded => 5,
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

impl FromStr for ExecutionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SUBMITTED" => Ok(ExecutionStatus::Submitted),
            "QUEUED" => Ok(ExecutionStatus::Queued),
            "RUNNING" => Ok(ExecutionStatus::Running),
            "SUCCEEDED" => Ok(ExecutionStatus::Succeeded),
            "FAILED" => Ok(ExecutionStatus::Failed),
            "CANCELLED" => Ok(ExecutionStatus::Cancelled),
            "TIMED_OUT" => Ok(ExecutionStatus::TimedOut),
            _ => Err(format!("Invalid ExecutionStatus: {}", s)),
        }
    }
}

impl TryFrom<i32> for ExecutionStatus {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ExecutionStatus::Submitted),
            1 => Ok(ExecutionStatus::Queued),
            2 => Ok(ExecutionStatus::Running),
            3 => Ok(ExecutionStatus::Succeeded),
            4 => Ok(ExecutionStatus::Failed),
            5 => Ok(ExecutionStatus::Cancelled),
            6 => Ok(ExecutionStatus::TimedOut),
            _ => Err(format!("Invalid ExecutionStatus value: {}", value)),
        }
    }
}

impl From<&ExecutionStatus> for i32 {
    fn from(status: &ExecutionStatus) -> Self {
        match status {
            ExecutionStatus::Submitted => 0,
            ExecutionStatus::Queued => 1,
            ExecutionStatus::Running => 2,
            ExecutionStatus::Succeeded => 3,
            ExecutionStatus::Failed => 4,
            ExecutionStatus::Cancelled => 5,
            ExecutionStatus::TimedOut => 6,
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

#[cfg(test)]
mod tests {
    use super::*;

    // JobState tests
    #[test]
    fn test_job_state_from_str() {
        assert_eq!("PENDING".parse::<JobState>().unwrap(), JobState::Pending);
        assert_eq!("ASSIGNED".parse::<JobState>().unwrap(), JobState::Assigned);
        assert_eq!(
            "SCHEDULED".parse::<JobState>().unwrap(),
            JobState::Scheduled
        );
        assert_eq!("RUNNING".parse::<JobState>().unwrap(), JobState::Running);
        assert_eq!(
            "SUCCEEDED".parse::<JobState>().unwrap(),
            JobState::Succeeded
        );
        assert_eq!("FAILED".parse::<JobState>().unwrap(), JobState::Failed);
        assert_eq!(
            "CANCELLED".parse::<JobState>().unwrap(),
            JobState::Cancelled
        );
        assert_eq!("TIMEOUT".parse::<JobState>().unwrap(), JobState::Timeout);

        assert!("INVALID".parse::<JobState>().is_err());
    }

    #[test]
    fn test_job_state_try_from_i32() {
        assert_eq!(JobState::try_from(0).unwrap(), JobState::Pending);
        assert_eq!(JobState::try_from(1).unwrap(), JobState::Assigned);
        assert_eq!(JobState::try_from(2).unwrap(), JobState::Scheduled);
        assert_eq!(JobState::try_from(3).unwrap(), JobState::Running);
        assert_eq!(JobState::try_from(4).unwrap(), JobState::Succeeded);
        assert_eq!(JobState::try_from(5).unwrap(), JobState::Failed);
        assert_eq!(JobState::try_from(6).unwrap(), JobState::Cancelled);
        assert_eq!(JobState::try_from(7).unwrap(), JobState::Timeout);

        assert!(JobState::try_from(99).is_err());
    }

    #[test]
    fn test_job_state_into_i32() {
        assert_eq!(i32::from(&JobState::Pending), 0);
        assert_eq!(i32::from(&JobState::Assigned), 1);
        assert_eq!(i32::from(&JobState::Scheduled), 2);
        assert_eq!(i32::from(&JobState::Running), 3);
        assert_eq!(i32::from(&JobState::Succeeded), 4);
        assert_eq!(i32::from(&JobState::Failed), 5);
        assert_eq!(i32::from(&JobState::Cancelled), 6);
        assert_eq!(i32::from(&JobState::Timeout), 7);
    }

    // WorkerState tests
    #[test]
    fn test_worker_state_from_str() {
        assert_eq!(
            "CREATING".parse::<WorkerState>().unwrap(),
            WorkerState::Creating
        );
        assert_eq!(
            "CONNECTING".parse::<WorkerState>().unwrap(),
            WorkerState::Connecting
        );
        assert_eq!("READY".parse::<WorkerState>().unwrap(), WorkerState::Ready);
        assert_eq!("BUSY".parse::<WorkerState>().unwrap(), WorkerState::Busy);
        assert_eq!(
            "DRAINING".parse::<WorkerState>().unwrap(),
            WorkerState::Draining
        );
        assert_eq!(
            "TERMINATING".parse::<WorkerState>().unwrap(),
            WorkerState::Terminating
        );
        assert_eq!(
            "TERMINATED".parse::<WorkerState>().unwrap(),
            WorkerState::Terminated
        );

        assert!("INVALID".parse::<WorkerState>().is_err());
    }

    #[test]
    fn test_worker_state_try_from_i32() {
        assert_eq!(WorkerState::try_from(0).unwrap(), WorkerState::Creating);
        assert_eq!(WorkerState::try_from(1).unwrap(), WorkerState::Connecting);
        assert_eq!(WorkerState::try_from(2).unwrap(), WorkerState::Ready);
        assert_eq!(WorkerState::try_from(3).unwrap(), WorkerState::Busy);
        assert_eq!(WorkerState::try_from(4).unwrap(), WorkerState::Draining);
        assert_eq!(WorkerState::try_from(5).unwrap(), WorkerState::Terminating);
        assert_eq!(WorkerState::try_from(6).unwrap(), WorkerState::Terminated);

        assert!(WorkerState::try_from(99).is_err());
    }

    #[test]
    fn test_worker_state_into_i32() {
        assert_eq!(i32::from(&WorkerState::Creating), 0);
        assert_eq!(i32::from(&WorkerState::Connecting), 1);
        assert_eq!(i32::from(&WorkerState::Ready), 2);
        assert_eq!(i32::from(&WorkerState::Busy), 3);
        assert_eq!(i32::from(&WorkerState::Draining), 4);
        assert_eq!(i32::from(&WorkerState::Terminating), 5);
        assert_eq!(i32::from(&WorkerState::Terminated), 6);
    }

    // ProviderStatus tests
    #[test]
    fn test_provider_status_from_str() {
        assert_eq!(
            "ACTIVE".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Active
        );
        assert_eq!(
            "MAINTENANCE".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Maintenance
        );
        assert_eq!(
            "DISABLED".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Disabled
        );
        assert_eq!(
            "OVERLOADED".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Overloaded
        );
        assert_eq!(
            "UNHEALTHY".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Unhealthy
        );
        assert_eq!(
            "DEGRADED".parse::<ProviderStatus>().unwrap(),
            ProviderStatus::Degraded
        );

        assert!("INVALID".parse::<ProviderStatus>().is_err());
    }

    #[test]
    fn test_provider_status_try_from_i32() {
        assert_eq!(ProviderStatus::try_from(0).unwrap(), ProviderStatus::Active);
        assert_eq!(
            ProviderStatus::try_from(1).unwrap(),
            ProviderStatus::Maintenance
        );
        assert_eq!(
            ProviderStatus::try_from(2).unwrap(),
            ProviderStatus::Disabled
        );
        assert_eq!(
            ProviderStatus::try_from(3).unwrap(),
            ProviderStatus::Overloaded
        );
        assert_eq!(
            ProviderStatus::try_from(4).unwrap(),
            ProviderStatus::Unhealthy
        );
        assert_eq!(
            ProviderStatus::try_from(5).unwrap(),
            ProviderStatus::Degraded
        );

        assert!(ProviderStatus::try_from(99).is_err());
    }

    #[test]
    fn test_provider_status_into_i32() {
        assert_eq!(i32::from(&ProviderStatus::Active), 0);
        assert_eq!(i32::from(&ProviderStatus::Maintenance), 1);
        assert_eq!(i32::from(&ProviderStatus::Disabled), 2);
        assert_eq!(i32::from(&ProviderStatus::Overloaded), 3);
        assert_eq!(i32::from(&ProviderStatus::Unhealthy), 4);
        assert_eq!(i32::from(&ProviderStatus::Degraded), 5);
    }

    // ExecutionStatus tests
    #[test]
    fn test_execution_status_from_str() {
        assert_eq!(
            "SUBMITTED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Submitted
        );
        assert_eq!(
            "QUEUED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Queued
        );
        assert_eq!(
            "RUNNING".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Running
        );
        assert_eq!(
            "SUCCEEDED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Succeeded
        );
        assert_eq!(
            "FAILED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Failed
        );
        assert_eq!(
            "CANCELLED".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::Cancelled
        );
        assert_eq!(
            "TIMED_OUT".parse::<ExecutionStatus>().unwrap(),
            ExecutionStatus::TimedOut
        );

        assert!("INVALID".parse::<ExecutionStatus>().is_err());
    }

    #[test]
    fn test_execution_status_try_from_i32() {
        assert_eq!(
            ExecutionStatus::try_from(0).unwrap(),
            ExecutionStatus::Submitted
        );
        assert_eq!(
            ExecutionStatus::try_from(1).unwrap(),
            ExecutionStatus::Queued
        );
        assert_eq!(
            ExecutionStatus::try_from(2).unwrap(),
            ExecutionStatus::Running
        );
        assert_eq!(
            ExecutionStatus::try_from(3).unwrap(),
            ExecutionStatus::Succeeded
        );
        assert_eq!(
            ExecutionStatus::try_from(4).unwrap(),
            ExecutionStatus::Failed
        );
        assert_eq!(
            ExecutionStatus::try_from(5).unwrap(),
            ExecutionStatus::Cancelled
        );
        assert_eq!(
            ExecutionStatus::try_from(6).unwrap(),
            ExecutionStatus::TimedOut
        );

        assert!(ExecutionStatus::try_from(99).is_err());
    }

    #[test]
    fn test_execution_status_into_i32() {
        assert_eq!(i32::from(&ExecutionStatus::Submitted), 0);
        assert_eq!(i32::from(&ExecutionStatus::Queued), 1);
        assert_eq!(i32::from(&ExecutionStatus::Running), 2);
        assert_eq!(i32::from(&ExecutionStatus::Succeeded), 3);
        assert_eq!(i32::from(&ExecutionStatus::Failed), 4);
        assert_eq!(i32::from(&ExecutionStatus::Cancelled), 5);
        assert_eq!(i32::from(&ExecutionStatus::TimedOut), 6);
    }
}
