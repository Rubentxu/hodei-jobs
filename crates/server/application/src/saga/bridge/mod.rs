//!
//! # Bridge Layer for EPIC-94 Migration
//!

pub mod command_bus;
pub mod domain_events;
pub mod job_state_machine;
pub mod worker_lifecycle;

pub use command_bus::{
    Activity, ActivityInput, ActivityOutput, CommandBusActivityConfig, CommandBusActivityError,
    CommandBusActivityRegistry,
};
pub use domain_events::{
    BridgeError, DomainEventBridgeConfig, SignalDispatcher, SignalNotification,
};
pub use job_state_machine::{
    AssignJobInput, AssignJobOutput, CancelJobInput, CancelJobOutput, CompleteJobInput,
    CompleteJobOutput, CreateJobInput, FailJobInput, JobStateMachineError, JobStateTransitionInput,
    JobStateTransitionOutput,
};
pub use worker_lifecycle::{
    BusyWorkerInput, RegisterWorkerInput, RegisterWorkerOutput, TerminateWorkerInput,
    TerminateWorkerOutput, WorkerLifecycleError, WorkerLifecycleInput, WorkerLifecycleOutput,
};
