// Saga Commands Module
//
// Provides command types used by sagas for worker lifecycle management.
// This enables saga steps to use the Command Bus pattern.
//
// NOTE: Handler implementations are in the application layer
// (application/src/saga/handlers/) following Hexagonal Architecture.
// This module only exports commands, errors, and traits.

pub mod cancellation;
pub mod execution;
pub mod provisioning;
pub mod recovery;
pub mod timeout;

pub use cancellation::{
    NotifyWorkerCommand, NotifyWorkerError, NotifyWorkerResult,
    ReleaseWorkerCommand, ReleaseWorkerError, ReleaseWorkerHandler, ReleaseWorkerResult,
    UpdateJobStateCommand, UpdateJobStateError, UpdateJobStateHandler, UpdateJobStateResult,
};
pub use execution::{
    AssignWorkerCommand, AssignWorkerError, CompleteJobCommand,
    CompleteJobError, ExecuteJobCommand, ExecuteJobError,
    ValidateJobCommand, ValidateJobError, WorkerAssignmentResult,
};
pub use provisioning::{
    CreateWorkerCommand, CreateWorkerError, CreateWorkerHandler, DestroyWorkerCommand,
    DestroyWorkerError, DestroyWorkerHandler, JobRequirements, ProviderCapacity,
    ProviderConfig, ProviderRegistry, PublishProvisionedCommand, PublishProvisionedError,
    UnregisterWorkerCommand, UnregisterWorkerError, UnregisterWorkerHandler, ValidateProviderCommand, ValidateProviderError,
};
pub use recovery::{
    CheckConnectivityCommand, CheckConnectivityResult, DestroyOldWorkerCommand,
    MarkJobForRecoveryCommand, ProvisionNewWorkerCommand, TransferJobCommand,
};
pub use timeout::{
    MarkJobTimedOutCommand, MarkJobTimedOutError, MarkJobTimedOutResult,
    TerminateWorkerCommand, TerminateWorkerError, TerminateWorkerResult,
};
