// Saga Commands Module
//
// Provides command types used by sagas for worker lifecycle management.
// This enables saga steps to use the Command Bus pattern.

pub mod cancellation;
pub mod execution;
pub mod provisioning;
pub mod recovery;
pub mod timeout;

pub use cancellation::{
    NotifyWorkerCommand, NotifyWorkerError, NotifyWorkerHandler, NotifyWorkerResult,
    ReleaseWorkerCommand, ReleaseWorkerError, ReleaseWorkerHandler, ReleaseWorkerResult,
    UpdateJobStateCommand, UpdateJobStateError, UpdateJobStateHandler, UpdateJobStateResult,
};
pub use execution::{
    AssignWorkerCommand, AssignWorkerError, AssignWorkerHandler, CompleteJobCommand,
    CompleteJobError, CompleteJobHandler, ExecuteJobCommand, ExecuteJobError, ExecuteJobHandler,
    ValidateJobCommand, ValidateJobError, ValidateJobHandler, WorkerAssignmentResult,
};
pub use provisioning::{
    CreateWorkerCommand, CreateWorkerError, CreateWorkerHandler, DestroyWorkerCommand,
    DestroyWorkerError, DestroyWorkerHandler, UnregisterWorkerCommand, UnregisterWorkerError,
    UnregisterWorkerHandler,
};
pub use recovery::{
    CheckConnectivityCommand, CheckConnectivityResult, DestroyOldWorkerCommand,
    MarkJobForRecoveryCommand, ProvisionNewWorkerCommand, TransferJobCommand,
};
pub use timeout::{
    MarkJobTimedOutCommand, MarkJobTimedOutError, MarkJobTimedOutHandler, MarkJobTimedOutResult,
    TerminateWorkerCommand, TerminateWorkerError, TerminateWorkerHandler, TerminateWorkerResult,
};
