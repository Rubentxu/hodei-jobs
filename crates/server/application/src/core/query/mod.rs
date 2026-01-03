//! Query Bus Infrastructure for CQRS Pattern
//!
//! Provides basic query infrastructure for the application layer.
//! This is a simplified implementation suitable for single-node deployments.

use hodei_server_domain::shared_kernel::{DomainError, Result};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// Pagination parameters for queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pagination {
    pub limit: u32,
    pub offset: u32,
}

impl Pagination {
    pub fn new(limit: u32, offset: u32) -> Self {
        Self { limit, offset }
    }

    pub fn with_limit(limit: u32) -> Self {
        Self { limit, offset: 0 }
    }

    pub fn validate(&self) -> Result<()> {
        if self.limit == 0 || self.limit > 1000 {
            return Err(DomainError::InvalidJobSpec {
                field: "pagination.limit".to_string(),
                reason: "Limit must be 1-1000".to_string(),
            });
        }
        Ok(())
    }
}

/// Paginated result wrapper
#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total_count: usize,
    pub limit: u32,
    pub offset: u32,
    pub has_next_page: bool,
}

impl<T> PaginatedResult<T> {
    pub fn new(items: Vec<T>, total_count: usize, offset: u32, limit: u32) -> Self {
        let items_len = items.len();
        let has_next_page = (offset + items_len as u32) < total_count as u32;
        Self {
            items,
            total_count,
            limit,
            offset,
            has_next_page,
        }
    }
}

/// Trait for all Queries
pub trait Query: Debug + Send + Sync + 'static {
    const NAME: &'static str;
    type Result;
}

/// Marker trait for queries that can be validated
pub trait ValidatableQuery: Query {
    fn validate(&self) -> Result<()>;
}

/// Query Handler trait
#[async_trait::async_trait]
pub trait QueryHandler<Q: Query>: Send + Sync {
    async fn handle(&self, query: Q) -> Result<Q::Result>;
}

/// Filter operators
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    Like,
}

#[derive(Debug, Clone)]
pub struct FilterCondition {
    pub field: String,
    pub operator: FilterOperator,
    pub value: serde_json::Value,
}

impl FilterCondition {
    pub fn eq<T: Into<serde_json::Value>>(field: impl Into<String>, value: T) -> Self {
        Self {
            field: field.into(),
            operator: FilterOperator::Equals,
            value: value.into(),
        }
    }
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Sort specification
#[derive(Debug, Clone)]
pub struct SortSpec {
    pub field: String,
    pub direction: SortDirection,
}

/// Query options
#[derive(Debug, Clone, Default)]
pub struct QueryOptions {
    pub filters: Vec<FilterCondition>,
    pub sorts: Vec<SortSpec>,
    pub pagination: Option<Pagination>,
}

impl QueryOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_filter(mut self, filter: FilterCondition) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn with_pagination(mut self, pagination: Pagination) -> Self {
        self.pagination = Some(pagination);
        self
    }
}

/// Simple in-memory query bus (placeholder for more complex implementations)
#[derive(Clone, Default)]
pub struct InMemoryQueryBus;

impl InMemoryQueryBus {
    pub fn new() -> Self {
        Self
    }
}

/// Read model port for templates
#[async_trait::async_trait]
pub trait TemplateReadModelPort: Send + Sync {
    async fn get_by_id(&self, id: &str) -> Option<serde_json::Value>;
    async fn list(
        &self,
        status: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<serde_json::Value>;
}

/// Read model port for executions
#[async_trait::async_trait]
pub trait ExecutionReadModelPort: Send + Sync {
    async fn get_by_id(&self, id: &str) -> Option<serde_json::Value>;
    async fn list_by_template(
        &self,
        template_id: &str,
        limit: usize,
        offset: usize,
    ) -> Vec<serde_json::Value>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestQuery {
        pub name: Option<String>,
    }

    impl Query for TestQuery {
        const NAME: &'static str = "TestQuery";
        type Result = Vec<String>;
    }

    #[tokio::test]
    async fn test_pagination_validation() {
        let pagination = Pagination::new(10, 0);
        assert!(pagination.validate().is_ok());

        let invalid = Pagination::new(0, 0);
        assert!(invalid.validate().is_err());
    }

    #[tokio::test]
    async fn test_paginated_result() {
        let result = PaginatedResult::new(vec!["a", "b"], 10, 0, 10);
        assert_eq!(result.items.len(), 2);
        assert!(result.has_next_page);
    }
}
