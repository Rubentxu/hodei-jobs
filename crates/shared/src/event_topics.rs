//! Centralized event topic constants for NATS JetStream
//!
//! This module provides a single source of truth for all event topic names,
//! preventing mismatches between publishers and consumers.
//!
//! ## Naming Convention
//! - Format: `hodei.events.{entity}.{action}`
//! - entity: The domain entity (jobs, workers, providers, etc.)
//! - action: The event type in lowercase (jobcreated, jobqueued, workerready, etc.)
//!
//! ## Usage
//! ```rust
//! use hodei_shared::event_topics::EVENT_PREFIX;
//!
//! assert_eq!(EVENT_PREFIX, "hodei.events");
//! ```

/// Subject prefix for all Hodei events
pub const EVENT_PREFIX: &str = "hodei.events";

/// Stream name for all events
pub const EVENTS_STREAM_NAME: &str = "HODEI_EVENTS";

/// Job-related event topics
pub mod job_topics {

    /// JobCreated event subject
    pub const CREATED: &str = "hodei.events.jobs.jobcreated";
    /// JobQueued event subject (triggers execution saga)
    pub const QUEUED: &str = "hodei.events.jobs.jobqueued";
    /// JobAssigned event subject
    pub const ASSIGNED: &str = "hodei.events.jobs.jobassigned";
    /// JobAccepted event subject
    pub const ACCEPTED: &str = "hodei.events.jobs.jobaccepted";
    /// JobStatusChanged event subject
    pub const STATUS_CHANGED: &str = "hodei.events.jobs.jobstatuschanged";
    /// JobRunning event subject
    pub const RUNNING: &str = "hodei.events.jobs.jobrunning";
    /// JobCompleted event subject
    pub const COMPLETED: &str = "hodei.events.jobs.jobcompleted";
    /// JobFailed event subject
    pub const FAILED: &str = "hodei.events.jobs.jobfailed";
    /// JobCancelled event subject
    pub const CANCELLED: &str = "hodei.events.jobs.jobcancelled";
    /// JobRetried event subject
    pub const RETRIED: &str = "hodei.events.jobs.jobretried";
    /// JobDispatchAcknowledged event subject
    pub const DISPATCH_ACKNOWLEDGED: &str = "hodei.events.jobs.jobdispatchacknowledged";
    /// JobDispatchFailed event subject
    pub const DISPATCH_FAILED: &str = "hodei.events.jobs.jobdispatchfailed";
    /// JobExecutionError event subject
    pub const EXECUTION_ERROR: &str = "hodei.events.jobs.jobexecutionerror";
    /// RunJobReceived event subject
    pub const RUN_JOB_RECEIVED: &str = "hodei.events.jobs.runjobreceived";

    /// Wildcard for all job events
    pub const ALL: &str = "hodei.events.jobs.>";
}

/// Worker-related event topics
pub mod worker_topics {

    /// WorkerRegistered event subject
    pub const REGISTERED: &str = "hodei.events.workers.workerregistered";
    /// WorkerReady event subject (triggers execution saga)
    pub const READY: &str = "hodei.events.workers.workerready";
    /// WorkerReadyForJob event subject
    pub const READY_FOR_JOB: &str = "hodei.events.workers.workerreadyforjob";
    /// WorkerStatusChanged event subject
    pub const STATUS_CHANGED: &str = "hodei.events.workers.workerstatuschanged";
    /// WorkerTerminated event subject (triggers cleanup saga)
    pub const TERMINATED: &str = "hodei.events.workers.workerterminated";
    /// WorkerProvisioned event subject
    pub const PROVISIONED: &str = "hodei.events.workers.workerprovisioned";
    /// WorkerProvisioningRequested event subject
    pub const PROVISIONING_REQUESTED: &str = "hodei.events.workers.workerprovisioningrequested";
    /// WorkerProvisioningError event subject
    pub const PROVISIONING_ERROR: &str = "hodei.events.workers.workerprovisioningerror";
    /// WorkerDisconnected event subject
    pub const DISCONNECTED: &str = "hodei.events.workers.workerdisconnected";
    /// WorkerReconnected event subject
    pub const RECONNECTED: &str = "hodei.events.workers.workerreconnected";
    /// WorkerHeartbeat event subject
    pub const HEARTBEAT: &str = "hodei.events.workers.workerheartbeat";
    /// WorkerHeartbeatMissed event subject
    pub const HEARTBEAT_MISSED: &str = "hodei.events.workers.workerheartbeatmissed";
    /// WorkerSelfTerminated event subject
    pub const SELF_TERMINATED: &str = "hodei.events.workers.workerselfterminated";
    /// WorkerRecoveryFailed event subject
    pub const RECOVERY_FAILED: &str = "hodei.events.workers.workerrecoveryfailed";
    /// WorkerEphemeralCreated event subject
    pub const EPHEMERAL_CREATED: &str = "hodei.events.workers.workerephemeralcreated";
    /// WorkerEphemeralReady event subject
    pub const EPHEMERAL_READY: &str = "hodei.events.workers.workerephemeralready";
    /// WorkerEphemeralTerminating event subject
    pub const EPHEMERAL_TERMINATING: &str = "hodei.events.workers.workerephemeralterminating";
    /// WorkerEphemeralTerminated event subject
    pub const EPHEMERAL_TERMINATED: &str = "hodei.events.workers.workerephemeralterminated";
    /// WorkerEphemeralCleanedUp event subject
    pub const EPHEMERAL_CLEANED_UP: &str = "hodei.events.workers.workerephemeralcleanedup";
    /// WorkerEphemeralIdle event subject
    pub const EPHEMERAL_IDLE: &str = "hodei.events.workers.workerephemeralidle";
    /// OrphanWorkerDetected event subject
    pub const ORPHAN_DETECTED: &str = "hodei.events.workers.orphanworkerdetected";

    /// Wildcard for all worker events
    pub const ALL: &str = "hodei.events.workers.>";
}

/// Provider-related event topics
pub mod provider_topics {

    /// ProviderRegistered event subject
    pub const REGISTERED: &str = "hodei.events.providers.providerregistered";
    /// ProviderUpdated event subject
    pub const UPDATED: &str = "hodei.events.providers.providerupdated";
    /// ProviderHealthChanged event subject
    pub const HEALTH_CHANGED: &str = "hodei.events.providers.providerhealthchanged";
    /// ProviderRecovered event subject
    pub const RECOVERED: &str = "hodei.events.providers.providerrecovered";
    /// ProviderSelected event subject
    pub const SELECTED: &str = "hodei.events.providers.providerselected";
    /// ProviderExecutionError event subject
    pub const EXECUTION_ERROR: &str = "hodei.events.providers.providerexecutionerror";
    /// AutoScalingTriggered event subject
    pub const AUTO_SCALING_TRIGGERED: &str = "hodei.events.providers.autoscalingtriggered";

    /// Wildcard for all provider events
    pub const ALL: &str = "hodei.events.providers.>";
}

/// Template-related event topics (EPIC-34)
pub mod template_topics {

    /// TemplateCreated event subject
    pub const CREATED: &str = "hodei.events.templates.templatecreated";
    /// TemplateUpdated event subject
    pub const UPDATED: &str = "hodei.events.templates.templateupdated";
    /// TemplateDisabled event subject
    pub const DISABLED: &str = "hodei.events.templates.templatedisabled";
    /// TemplateRunCreated event subject
    pub const RUN_CREATED: &str = "hodei.events.templates.templateruncreated";
    /// ExecutionRecorded event subject
    pub const EXECUTION_RECORDED: &str = "hodei.events.templates.executionrecorded";

    /// Wildcard for all template events
    pub const ALL: &str = "hodei.events.templates.>";
}

/// Scheduling-related event topics (EPIC-34)
pub mod scheduling_topics {

    /// ScheduledJobCreated event subject
    pub const JOB_CREATED: &str = "hodei.events.scheduling.scheduledjobcreated";
    /// ScheduledJobTriggered event subject
    pub const JOB_TRIGGERED: &str = "hodei.events.scheduling.scheduledjobtriggered";
    /// ScheduledJobMissed event subject
    pub const JOB_MISSED: &str = "hodei.events.scheduling.scheduledjobmissed";
    /// ScheduledJobError event subject
    pub const JOB_ERROR: &str = "hodei.events.scheduling.scheduledjoberror";
    /// SchedulingDecisionFailed event subject
    pub const DECISION_FAILED: &str = "hodei.events.scheduling.schedulingdecisionfailed";

    /// Wildcard for all scheduling events
    pub const ALL: &str = "hodei.events.scheduling.>";
}

/// Other system event topics
pub mod system_topics {

    /// JobQueueDepthChanged event subject
    pub const QUEUE_DEPTH_CHANGED: &str = "hodei.events.queue.jobqueuedepthchanged";
    /// GarbageCollectionCompleted event subject
    pub const GC_COMPLETED: &str = "hodei.events.gc.garbagecollectioncompleted";

    /// Wildcard for all queue events
    pub const QUEUE_ALL: &str = "hodei.events.queue.>";
    /// Wildcard for all GC events
    pub const GC_ALL: &str = "hodei.events.gc.>";
}

/// Wildcard for all Hodei events (multi-level)
pub const ALL_EVENTS: &str = "hodei.events.>";

/// Helper function to build event subject from entity and action
#[inline]
pub fn event_subject(entity: &str, action: &str) -> String {
    format!("{}.{}.{}", EVENT_PREFIX, entity, action)
}

#[cfg(test)]
mod tests {
    use crate::event_topics::{
        ALL_EVENTS, event_subject, job_topics, provider_topics, worker_topics,
    };

    #[test]
    fn test_event_topics_format() {
        // Verify all topics follow the expected format
        assert!(job_topics::CREATED.starts_with("hodei.events."));
        assert!(job_topics::CREATED.ends_with(".jobcreated"));
        assert!(worker_topics::READY.ends_with(".workerready"));
        assert!(provider_topics::SELECTED.ends_with(".providerselected"));
    }

    #[test]
    fn test_wildcards() {
        // Wildcards should use multi-level matching
        assert!(job_topics::ALL.ends_with(".>"));
        assert!(worker_topics::ALL.ends_with(".>"));
        assert!(ALL_EVENTS.ends_with(".>"));
    }

    #[test]
    fn test_event_subject_helper() {
        let subject = event_subject("jobs", "jobcreated");
        assert_eq!(subject, "hodei.events.jobs.jobcreated");

        let subject = event_subject("workers", "workerready");
        assert_eq!(subject, "hodei.events.workers.workerready");
    }
}
