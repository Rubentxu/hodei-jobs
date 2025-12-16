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

#[async_trait::async_trait]
pub trait AuditRepository: Send + Sync {
    async fn save(&self, log: &AuditLog) -> Result<()>;
    async fn find_by_correlation_id(&self, id: &str) -> Result<Vec<AuditLog>>;
}
