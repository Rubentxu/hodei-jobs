//! Audit gRPC Service Implementation
//!
//! Provides access to audit logs for compliance and debugging.
//! Story 15.8: Endpoint gRPC para Consultar Audit Logs

use std::sync::Arc;

use chrono::{DateTime, Utc};
use hodei_jobs::{
    audit_service_server::AuditService as AuditServiceTrait,
    AuditEventTypeCount, AuditLogEntry, GetAuditLogsByCorrelationRequest,
    GetAuditLogsRequest, GetAuditLogsResponse, GetEventCountsRequest, GetEventCountsResponse,
};
use hodei_server_domain::audit::{AuditQuery, AuditRepository};
use prost_types::Timestamp;
use tonic::{Request, Response, Status};

/// Maximum allowed limit for queries (to prevent abuse)
const MAX_LIMIT: i32 = 1000;
/// Default limit if not specified
const DEFAULT_LIMIT: i32 = 100;

/// gRPC service implementation for Audit
pub struct AuditServiceImpl {
    repository: Arc<dyn AuditRepository>,
}

impl AuditServiceImpl {
    pub fn new(repository: Arc<dyn AuditRepository>) -> Self {
        Self { repository }
    }
}

fn datetime_to_timestamp(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

fn timestamp_to_datetime(ts: &Timestamp) -> DateTime<Utc> {
    DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
        .unwrap_or_else(Utc::now)
}

fn audit_log_to_proto(log: &hodei_server_domain::audit::AuditLog) -> AuditLogEntry {
    AuditLogEntry {
        id: log.id.to_string(),
        correlation_id: log.correlation_id.clone(),
        event_type: log.event_type.clone(),
        payload_json: log.payload.to_string(),
        occurred_at: Some(datetime_to_timestamp(log.occurred_at)),
        actor: log.actor.clone(),
    }
}

#[tonic::async_trait]
impl AuditServiceTrait for AuditServiceImpl {
    async fn get_audit_logs(
        &self,
        request: Request<GetAuditLogsRequest>,
    ) -> Result<Response<GetAuditLogsResponse>, Status> {
        let req = request.into_inner();

        // Validate and normalize limit
        let limit = if req.limit <= 0 {
            DEFAULT_LIMIT
        } else if req.limit > MAX_LIMIT {
            MAX_LIMIT
        } else {
            req.limit
        };
        let offset = req.offset.max(0);

        // Build query
        let mut query = AuditQuery::new().with_pagination(limit as i64, offset as i64);

        if let Some(event_type) = req.event_type {
            if !event_type.is_empty() {
                query = query.with_event_type(event_type);
            }
        }

        if let Some(actor) = req.actor {
            if !actor.is_empty() {
                query = query.with_actor(actor);
            }
        }

        if let (Some(start), Some(end)) = (req.start_time.as_ref(), req.end_time.as_ref()) {
            query = query.with_date_range(
                timestamp_to_datetime(start),
                timestamp_to_datetime(end),
            );
        } else if let Some(start) = req.start_time.as_ref() {
            query.start_time = Some(timestamp_to_datetime(start));
        } else if let Some(end) = req.end_time.as_ref() {
            query.end_time = Some(timestamp_to_datetime(end));
        }

        // Execute query
        let result = self
            .repository
            .query(query)
            .await
            .map_err(|e| Status::internal(format!("Failed to query audit logs: {}", e)))?;

        let logs: Vec<AuditLogEntry> = result.logs.iter().map(audit_log_to_proto).collect();

        Ok(Response::new(GetAuditLogsResponse {
            logs,
            total_count: result.total_count,
            has_more: result.has_more,
        }))
    }

    async fn get_audit_logs_by_correlation(
        &self,
        request: Request<GetAuditLogsByCorrelationRequest>,
    ) -> Result<Response<GetAuditLogsResponse>, Status> {
        let req = request.into_inner();

        if req.correlation_id.is_empty() {
            return Err(Status::invalid_argument("correlation_id is required"));
        }

        let logs = self
            .repository
            .find_by_correlation_id(&req.correlation_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to find audit logs: {}", e)))?;

        let proto_logs: Vec<AuditLogEntry> = logs.iter().map(audit_log_to_proto).collect();
        let total_count = proto_logs.len() as i64;

        Ok(Response::new(GetAuditLogsResponse {
            logs: proto_logs,
            total_count,
            has_more: false,
        }))
    }

    async fn get_event_counts(
        &self,
        _request: Request<GetEventCountsRequest>,
    ) -> Result<Response<GetEventCountsResponse>, Status> {
        let counts = self
            .repository
            .count_by_event_type()
            .await
            .map_err(|e| Status::internal(format!("Failed to count events: {}", e)))?;

        let total_events: i64 = counts.iter().map(|c| c.count).sum();

        let proto_counts: Vec<AuditEventTypeCount> = counts
            .into_iter()
            .map(|c| AuditEventTypeCount {
                event_type: c.event_type,
                count: c.count,
            })
            .collect();

        Ok(Response::new(GetEventCountsResponse {
            counts: proto_counts,
            total_events,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::audit::{AuditLog, AuditQueryResult, EventTypeCount};
    use serde_json::json;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockAuditRepository {
        logs: Mutex<Vec<AuditLog>>,
    }

    impl MockAuditRepository {
        fn new() -> Self {
            Self {
                logs: Mutex::new(Vec::new()),
            }
        }

        fn with_logs(logs: Vec<AuditLog>) -> Self {
            Self {
                logs: Mutex::new(logs),
            }
        }
    }

    #[async_trait::async_trait]
    impl AuditRepository for MockAuditRepository {
        async fn save(&self, log: &AuditLog) -> hodei_server_domain::shared_kernel::Result<()> {
            self.logs.lock().unwrap().push(log.clone());
            Ok(())
        }

        async fn find_by_correlation_id(
            &self,
            id: &str,
        ) -> hodei_server_domain::shared_kernel::Result<Vec<AuditLog>> {
            let logs = self.logs.lock().unwrap();
            Ok(logs
                .iter()
                .filter(|l| l.correlation_id.as_deref() == Some(id))
                .cloned()
                .collect())
        }

        async fn find_by_event_type(
            &self,
            event_type: &str,
            limit: i64,
            offset: i64,
        ) -> hodei_server_domain::shared_kernel::Result<AuditQueryResult> {
            let logs = self.logs.lock().unwrap();
            let filtered: Vec<_> = logs
                .iter()
                .filter(|l| l.event_type == event_type)
                .cloned()
                .collect();
            let total = filtered.len() as i64;
            let result: Vec<_> = filtered
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
            Ok(AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len() as i64) < total,
            })
        }

        async fn find_by_date_range(
            &self,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
            limit: i64,
            offset: i64,
        ) -> hodei_server_domain::shared_kernel::Result<AuditQueryResult> {
            let logs = self.logs.lock().unwrap();
            let filtered: Vec<_> = logs
                .iter()
                .filter(|l| l.occurred_at >= start && l.occurred_at <= end)
                .cloned()
                .collect();
            let total = filtered.len() as i64;
            let result: Vec<_> = filtered
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
            Ok(AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len() as i64) < total,
            })
        }

        async fn find_by_actor(
            &self,
            actor: &str,
            limit: i64,
            offset: i64,
        ) -> hodei_server_domain::shared_kernel::Result<AuditQueryResult> {
            let logs = self.logs.lock().unwrap();
            let filtered: Vec<_> = logs
                .iter()
                .filter(|l| l.actor.as_deref() == Some(actor))
                .cloned()
                .collect();
            let total = filtered.len() as i64;
            let result: Vec<_> = filtered
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
            Ok(AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len() as i64) < total,
            })
        }

        async fn query(
            &self,
            query: AuditQuery,
        ) -> hodei_server_domain::shared_kernel::Result<AuditQueryResult> {
            let logs = self.logs.lock().unwrap();
            let mut filtered: Vec<_> = logs.iter().cloned().collect();

            if let Some(ref et) = query.event_type {
                filtered.retain(|l| &l.event_type == et);
            }
            if let Some(ref a) = query.actor {
                filtered.retain(|l| l.actor.as_deref() == Some(a.as_str()));
            }
            if let Some(start) = query.start_time {
                filtered.retain(|l| l.occurred_at >= start);
            }
            if let Some(end) = query.end_time {
                filtered.retain(|l| l.occurred_at <= end);
            }

            let total = filtered.len() as i64;
            let limit = query.limit.unwrap_or(100) as usize;
            let offset = query.offset.unwrap_or(0) as usize;
            let result: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

            Ok(AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len()) < total as usize,
            })
        }

        async fn count_by_event_type(
            &self,
        ) -> hodei_server_domain::shared_kernel::Result<Vec<EventTypeCount>> {
            let logs = self.logs.lock().unwrap();
            let mut counts: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            for log in logs.iter() {
                *counts.entry(log.event_type.clone()).or_insert(0) += 1;
            }
            Ok(counts
                .into_iter()
                .map(|(event_type, count)| EventTypeCount { event_type, count })
                .collect())
        }

        async fn delete_before(
            &self,
            before: DateTime<Utc>,
        ) -> hodei_server_domain::shared_kernel::Result<u64> {
            let mut logs = self.logs.lock().unwrap();
            let original_len = logs.len();
            logs.retain(|l| l.occurred_at >= before);
            Ok((original_len - logs.len()) as u64)
        }
    }

    fn create_test_log(event_type: &str, correlation_id: Option<&str>, actor: Option<&str>) -> AuditLog {
        AuditLog {
            id: Uuid::new_v4(),
            correlation_id: correlation_id.map(String::from),
            event_type: event_type.to_string(),
            payload: json!({"test": true}),
            occurred_at: Utc::now(),
            actor: actor.map(String::from),
        }
    }

    #[tokio::test]
    async fn test_get_audit_logs_empty() {
        let repo = Arc::new(MockAuditRepository::new());
        let service = AuditServiceImpl::new(repo);

        let request = Request::new(GetAuditLogsRequest {
            event_type: None,
            actor: None,
            start_time: None,
            end_time: None,
            limit: 10,
            offset: 0,
        });

        let response = service.get_audit_logs(request).await.unwrap();
        let inner = response.into_inner();

        assert_eq!(inner.logs.len(), 0);
        assert_eq!(inner.total_count, 0);
        assert!(!inner.has_more);
    }

    #[tokio::test]
    async fn test_get_audit_logs_with_filter() {
        let logs = vec![
            create_test_log("JobCreated", Some("corr-1"), Some("user1")),
            create_test_log("JobCompleted", Some("corr-1"), Some("user1")),
            create_test_log("WorkerRegistered", Some("corr-2"), Some("user2")),
        ];
        let repo = Arc::new(MockAuditRepository::with_logs(logs));
        let service = AuditServiceImpl::new(repo);

        let request = Request::new(GetAuditLogsRequest {
            event_type: Some("JobCreated".to_string()),
            actor: None,
            start_time: None,
            end_time: None,
            limit: 10,
            offset: 0,
        });

        let response = service.get_audit_logs(request).await.unwrap();
        let inner = response.into_inner();

        assert_eq!(inner.logs.len(), 1);
        assert_eq!(inner.logs[0].event_type, "JobCreated");
    }

    #[tokio::test]
    async fn test_get_audit_logs_by_correlation() {
        let logs = vec![
            create_test_log("JobCreated", Some("corr-1"), Some("user1")),
            create_test_log("JobCompleted", Some("corr-1"), Some("user1")),
            create_test_log("WorkerRegistered", Some("corr-2"), Some("user2")),
        ];
        let repo = Arc::new(MockAuditRepository::with_logs(logs));
        let service = AuditServiceImpl::new(repo);

        let request = Request::new(GetAuditLogsByCorrelationRequest {
            correlation_id: "corr-1".to_string(),
        });

        let response = service.get_audit_logs_by_correlation(request).await.unwrap();
        let inner = response.into_inner();

        assert_eq!(inner.logs.len(), 2);
        assert_eq!(inner.total_count, 2);
    }

    #[tokio::test]
    async fn test_get_event_counts() {
        let logs = vec![
            create_test_log("JobCreated", None, None),
            create_test_log("JobCreated", None, None),
            create_test_log("JobCompleted", None, None),
            create_test_log("WorkerRegistered", None, None),
        ];
        let repo = Arc::new(MockAuditRepository::with_logs(logs));
        let service = AuditServiceImpl::new(repo);

        let request = Request::new(GetEventCountsRequest {
            start_time: None,
            end_time: None,
        });

        let response = service.get_event_counts(request).await.unwrap();
        let inner = response.into_inner();

        assert_eq!(inner.total_events, 4);
        assert_eq!(inner.counts.len(), 3);
    }
}
