// Saga Commands Module
//
// Provides command types used by sagas for worker lifecycle management.
// This enables saga steps to use the Command Bus pattern.

pub mod execution;
pub mod provisioning;
pub mod recovery;

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
