//! Template Domain Events (EPIC-34)
//!
//! Events related to job templates and scheduled jobs.
//! This module implements the Template bounded context for domain events.

use crate::shared_kernel::JobId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// EPIC-34: A new job template has been created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateCreated {
    /// ID of the template
    pub template_id: String,
    /// Template name
    pub template_name: String,
    /// Template version
    pub version: u32,
    /// User who created it
    pub created_by: Option<String>,
    /// Summary of the spec
    pub spec_summary: String,
    /// When the template was created
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A job template has been updated
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateUpdated {
    /// ID of the template
    pub template_id: String,
    /// Template name
    pub template_name: String,
    /// Old version
    pub old_version: u32,
    /// New version
    pub new_version: u32,
    /// Changes made
    pub changes: Option<String>,
    /// When the template was updated
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A job template has been disabled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateDisabled {
    /// ID of the template
    pub template_id: String,
    /// Template name
    pub template_name: String,
    /// Version at time of disabling
    pub version: u32,
    /// When the template was disabled
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A new run has been created from a template
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateRunCreated {
    /// ID of the template
    pub template_id: String,
    /// Template name
    pub template_name: String,
    /// Execution ID
    pub execution_id: String,
    /// Job ID
    pub job_id: Option<JobId>,
    /// Job name
    pub job_name: String,
    /// Version of template used
    pub template_version: u32,
    /// Execution number
    pub execution_number: u64,
    /// What triggered this run
    pub triggered_by: String,
    /// User who triggered (if applicable)
    pub triggered_by_user: Option<String>,
    /// When the run was created
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A job execution has been recorded
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionRecorded {
    /// ID of the execution
    pub execution_id: String,
    /// Template ID
    pub template_id: String,
    /// Job ID
    pub job_id: Option<JobId>,
    /// Execution status
    pub status: String,
    /// Exit code (if completed)
    pub exit_code: Option<i32>,
    /// Duration in ms
    pub duration_ms: Option<u64>,
    /// When the execution was recorded
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A scheduled job has been created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledJobCreated {
    /// ID of the scheduled job
    pub scheduled_job_id: String,
    /// Name of the scheduled job
    pub name: String,
    /// Template ID
    pub template_id: String,
    /// Cron expression
    pub cron_expression: String,
    /// Timezone
    pub timezone: String,
    /// User who created it
    pub created_by: Option<String>,
    /// When the scheduled job was created
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A scheduled job has been triggered
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledJobTriggered {
    /// ID of the scheduled job
    pub scheduled_job_id: String,
    /// Name of the scheduled job
    pub name: String,
    /// Template ID
    pub template_id: String,
    /// Execution ID
    pub execution_id: String,
    /// Job ID
    pub job_id: Option<JobId>,
    /// When it was scheduled for
    pub scheduled_for: DateTime<Utc>,
    /// When it was actually triggered
    pub triggered_at: DateTime<Utc>,
    /// Parameters used
    pub parameters: HashMap<String, String>,
    /// When the trigger occurred
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A scheduled job execution was missed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledJobMissed {
    /// ID of the scheduled job
    pub scheduled_job_id: String,
    /// Name of the scheduled job
    pub name: String,
    /// When it should have run
    pub scheduled_for: DateTime<Utc>,
    /// When it was detected as missed
    pub detected_at: DateTime<Utc>,
    /// Reason for missing
    pub reason: String,
    /// When the miss was detected
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

/// EPIC-34: A scheduled job encountered an error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledJobError {
    /// ID of the scheduled job
    pub scheduled_job_id: String,
    /// Name of the scheduled job
    pub name: String,
    /// Error message
    pub error_message: String,
    /// Execution ID if available
    pub execution_id: Option<String>,
    /// When the error occurred
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor who performed the action
    pub actor: Option<String>,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::JobId;

    #[test]
    fn test_template_created_event() {
        let event = TemplateCreated {
            template_id: "template-123".to_string(),
            template_name: "test-template".to_string(),
            version: 1,
            created_by: Some("admin".to_string()),
            spec_summary: "Echo hello world".to_string(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: Some("system".to_string()),
        };

        assert_eq!(event.template_id, "template-123");
        assert_eq!(event.version, 1);
        assert_eq!(event.actor, Some("system".to_string()));
    }

    #[test]
    fn test_scheduled_job_triggered_event() {
        let scheduled_for = Utc::now();
        let triggered_at = scheduled_for + chrono::Duration::seconds(5);

        let event = ScheduledJobTriggered {
            scheduled_job_id: "scheduled-123".to_string(),
            name: "daily-cleanup".to_string(),
            template_id: "template-456".to_string(),
            execution_id: "exec-789".to_string(),
            job_id: Some(JobId::new()),
            scheduled_for,
            triggered_at,
            parameters: HashMap::new(),
            occurred_at: triggered_at,
            correlation_id: Some("corr-123".to_string()),
            actor: Some("scheduler".to_string()),
        };

        assert_eq!(event.name, "daily-cleanup");
        assert!(event.job_id.is_some());
        assert_eq!(event.correlation_id, Some("corr-123".to_string()));
    }

    #[test]
    fn test_execution_recorded_serialization() {
        let event = ExecutionRecorded {
            execution_id: "exec-123".to_string(),
            template_id: "template-456".to_string(),
            job_id: Some(JobId::new()),
            status: "completed".to_string(),
            exit_code: Some(0),
            duration_ms: Some(1500),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        // Test serialization/deserialization roundtrip
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: ExecutionRecorded = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.execution_id, event.execution_id);
        assert_eq!(deserialized.status, event.status);
        assert_eq!(deserialized.exit_code, event.exit_code);
    }
}
