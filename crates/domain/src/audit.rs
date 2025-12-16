use crate::shared_kernel::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditLog {
    pub id: Uuid,
    pub correlation_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
    pub actor: Option<String>,
}

impl AuditLog {
    pub fn new(
        event_type: String,
        payload: Value,
        correlation_id: Option<String>,
        actor: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id,
            event_type,
            payload,
            occurred_at: Utc::now(),
            actor,
        }
    }
}

/// Query parameters for audit log searches
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    pub event_type: Option<String>,
    pub actor: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl AuditQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    pub fn with_date_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    pub fn with_pagination(mut self, limit: i64, offset: i64) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Result of a paginated audit query
#[derive(Debug, Clone)]
pub struct AuditQueryResult {
    pub logs: Vec<AuditLog>,
    pub total_count: i64,
    pub has_more: bool,
}

/// Count of events by type
#[derive(Debug, Clone)]
pub struct EventTypeCount {
    pub event_type: String,
    pub count: i64,
}

#[async_trait::async_trait]
pub trait AuditRepository: Send + Sync {
    /// Save an audit log entry
    async fn save(&self, log: &AuditLog) -> Result<()>;
    
    /// Find audit logs by correlation ID
    async fn find_by_correlation_id(&self, id: &str) -> Result<Vec<AuditLog>>;
    
    /// Find audit logs by event type with pagination
    async fn find_by_event_type(
        &self,
        event_type: &str,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult>;
    
    /// Find audit logs by date range
    async fn find_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult>;
    
    /// Find audit logs by actor
    async fn find_by_actor(
        &self,
        actor: &str,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult>;
    
    /// Advanced query with multiple filters
    async fn query(&self, query: AuditQuery) -> Result<AuditQueryResult>;
    
    /// Count events by type
    async fn count_by_event_type(&self) -> Result<Vec<EventTypeCount>>;
    
    /// Delete audit logs older than the specified date (for retention policy)
    async fn delete_before(&self, before: DateTime<Utc>) -> Result<u64>;
}
