//!
//! # Workflow Migration Checklist
//!
//! Provides a structured checklist for validating the complete migration
//! of workflows from legacy saga system to saga-engine v4.
//!

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents a migration checklist item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub category: ChecklistCategory,
    pub description: String,
    pub is_required: bool,
    pub is_checked: bool,
    pub checked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub checked_by: Option<String>,
    pub notes: Option<String>,
    pub evidence: Vec<String>,
}

impl ChecklistItem {
    pub fn new(
        id: String,
        category: ChecklistCategory,
        description: String,
        is_required: bool,
    ) -> Self {
        Self {
            id,
            category,
            description,
            is_required,
            is_checked: false,
            checked_at: None,
            checked_by: None,
            notes: None,
            evidence: Vec::new(),
        }
    }

    pub fn check(&mut self, checked_by: String, notes: Option<String>) {
        self.is_checked = true;
        self.checked_at = Some(chrono::Utc::now());
        self.checked_by = Some(checked_by);
        self.notes = notes;
    }

    pub fn uncheck(&mut self) {
        self.is_checked = false;
        self.checked_at = None;
        self.checked_by = None;
        self.notes = None;
    }

    pub fn add_evidence(&mut self, evidence: String) {
        self.evidence.push(evidence);
    }
}

/// Categories for checklist items
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChecklistCategory {
    InputValidation,
    OutputDefinition,
    ActivityImplementation,
    CompensationLogic,
    EventEmission,
    TestCoverage,
    Documentation,
    Performance,
    Security,
    Observability,
}

/// Checklist template for a specific workflow type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowChecklist {
    pub workflow_type: String,
    pub workflow_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub owner: Option<String>,
    pub status: ChecklistStatus,
    pub items: Vec<ChecklistItem>,
    pub overall_notes: Vec<String>,
}

impl WorkflowChecklist {
    pub fn new(workflow_type: &str, workflow_name: &str) -> Self {
        let items = Self::default_items(workflow_type);

        Self {
            workflow_type: workflow_type.to_string(),
            workflow_name: workflow_name.to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            owner: None,
            status: ChecklistStatus::InProgress,
            items,
            overall_notes: Vec::new(),
        }
    }

    /// Returns default checklist items for a workflow type
    fn default_items(workflow_type: &str) -> Vec<ChecklistItem> {
        let mut items = Vec::new();
        let prefix = format!("{}-", workflow_type.to_lowercase().replace(' ', "-"));

        // Input Validation items
        items.push(ChecklistItem::new(
            format!("{}input-001", prefix),
            ChecklistCategory::InputValidation,
            "All input types are properly defined with validation".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}input-002", prefix),
            ChecklistCategory::InputValidation,
            "Input deserialization handles all legacy input formats".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}input-003", prefix),
            ChecklistCategory::InputValidation,
            "Idempotency key handling matches legacy behavior".to_string(),
            true,
        ));

        // Output Definition items
        items.push(ChecklistItem::new(
            format!("{}output-001", prefix),
            ChecklistCategory::OutputDefinition,
            "Output type includes all data returned by legacy saga".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}output-002", prefix),
            ChecklistCategory::OutputDefinition,
            "Output serialization is backward compatible".to_string(),
            true,
        ));

        // Activity Implementation items
        items.push(ChecklistItem::new(
            format!("{}activity-001", prefix),
            ChecklistCategory::ActivityImplementation,
            "All legacy steps have equivalent v4 activities".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}activity-002", prefix),
            ChecklistCategory::ActivityImplementation,
            "Activity error handling matches legacy error types".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}activity-003", prefix),
            ChecklistCategory::ActivityImplementation,
            "Activity timeouts match or exceed legacy timeouts".to_string(),
            true,
        ));

        // Compensation Logic items
        items.push(ChecklistItem::new(
            format!("{}compensate-001", prefix),
            ChecklistCategory::CompensationLogic,
            "All destructive operations have compensation handlers".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}compensate-002", prefix),
            ChecklistCategory::CompensationLogic,
            "Compensation order is correct (reverse execution order)".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}compensate-003", prefix),
            ChecklistCategory::CompensationLogic,
            "Compensation errors are handled appropriately".to_string(),
            true,
        ));

        // Event Emission items
        items.push(ChecklistItem::new(
            format!("{}events-001", prefix),
            ChecklistCategory::EventEmission,
            "All legacy domain events are emitted by v4 workflow".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}events-002", prefix),
            ChecklistCategory::EventEmission,
            "Event payload structure matches legacy format".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}events-003", prefix),
            ChecklistCategory::EventEmission,
            "Event ordering is preserved (same as legacy)".to_string(),
            true,
        ));

        // Test Coverage items
        items.push(ChecklistItem::new(
            format!("{}test-001", prefix),
            ChecklistCategory::TestCoverage,
            "Equivalence tests pass with 100% success rate".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}test-002", prefix),
            ChecklistCategory::TestCoverage,
            "Error path tests cover all failure scenarios".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}test-003", prefix),
            ChecklistCategory::TestCoverage,
            "Compensation tests verify correct rollback behavior".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}test-004", prefix),
            ChecklistCategory::TestCoverage,
            "Integration tests verify end-to-end workflow".to_string(),
            true,
        ));

        // Documentation items
        items.push(ChecklistItem::new(
            format!("{}docs-001", prefix),
            ChecklistCategory::Documentation,
            "KDoc documentation for all public types".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}docs-002", prefix),
            ChecklistCategory::Documentation,
            "Migration decisions documented in ADR format".to_string(),
            false,
        ));
        items.push(ChecklistItem::new(
            format!("{}docs-003", prefix),
            ChecklistCategory::Documentation,
            "User-facing documentation updated for new workflow".to_string(),
            false,
        ));

        // Performance items
        items.push(ChecklistItem::new(
            format!("{}perf-001", prefix),
            ChecklistCategory::Performance,
            "Performance benchmarks show no regression".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}perf-002", prefix),
            ChecklistCategory::Performance,
            "Concurrency handling matches or exceeds legacy".to_string(),
            true,
        ));

        // Security items
        items.push(ChecklistItem::new(
            format!("{}security-001", prefix),
            ChecklistCategory::Security,
            "Input validation prevents injection attacks".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}security-002", prefix),
            ChecklistCategory::Security,
            "Secrets handling follows security policy".to_string(),
            true,
        ));

        // Observability items
        items.push(ChecklistItem::new(
            format!("{}obs-001", prefix),
            ChecklistCategory::Observability,
            "Tracing spans cover all workflow steps".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}obs-002", prefix),
            ChecklistCategory::Observability,
            "Metrics exported for workflow execution".to_string(),
            true,
        ));
        items.push(ChecklistItem::new(
            format!("{}obs-003", prefix),
            ChecklistCategory::Observability,
            "Logs include sufficient context for debugging".to_string(),
            true,
        ));

        items
    }

    /// Get progress percentage
    pub fn progress(&self) -> f64 {
        if self.items.is_empty() {
            return 100.0;
        }
        let checked = self.items.iter().filter(|i| i.is_checked).count();
        (checked as f64 / self.items.len() as f64) * 100.0
    }

    /// Check if all required items are checked
    pub fn all_required_checked(&self) -> bool {
        self.items
            .iter()
            .filter(|i| i.is_required)
            .all(|i| i.is_checked)
    }

    /// Get checklist status
    pub fn status(&self) -> ChecklistStatus {
        let required_checked = self.all_required_checked();
        let optional_checked = self
            .items
            .iter()
            .filter(|i| !i.is_required)
            .filter(|i| i.is_checked)
            .count();
        let total = self.items.iter().filter(|i| !i.is_required).count();

        if required_checked && optional_checked == total {
            ChecklistStatus::Completed
        } else if required_checked {
            ChecklistStatus::ReadyForSignoff
        } else {
            ChecklistStatus::InProgress
        }
    }

    /// Set owner
    pub fn set_owner(&mut self, owner: String) {
        self.owner = Some(owner);
        self.updated_at = chrono::Utc::now();
    }

    /// Add overall note
    pub fn add_note(&mut self, note: String) {
        self.overall_notes.push(note);
        self.updated_at = chrono::Utc::now();
    }

    /// Find item by ID
    pub fn find_item(&self, id: &str) -> Option<&ChecklistItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Find and mutate item by ID
    pub fn find_item_mut(&mut self, id: &str) -> Option<&mut ChecklistItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }
}

/// Status of a checklist
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChecklistStatus {
    NotStarted,
    InProgress,
    ReadyForSignoff,
    Completed,
    Blocked,
}

/// Result of checklist validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistValidationResult {
    pub checklist_type: String,
    pub is_valid: bool,
    pub required_items_pending: Vec<String>,
    pub optional_items_pending: usize,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Service for managing migration checklists
#[derive(Debug, Default)]
pub struct MigrationChecklistService {
    checklists: Arc<RwLock<HashMap<String, WorkflowChecklist>>>,
    completed_checklists: Arc<RwLock<Vec<WorkflowChecklist>>>,
}

impl MigrationChecklistService {
    pub fn new() -> Self {
        Self {
            checklists: Arc::new(RwLock::new(HashMap::new())),
            completed_checklists: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create a new checklist for a workflow type
    pub async fn create_checklist(&self, workflow_type: &str, workflow_name: &str) -> String {
        let checklist = WorkflowChecklist::new(workflow_type, workflow_name);
        let key = format!("{}:{}", workflow_type, workflow_name);

        let mut checklists = self.checklists.write().await;
        checklists.insert(key.clone(), checklist);

        key
    }

    /// Get a checklist by key
    pub async fn get_checklist(&self, key: &str) -> Option<WorkflowChecklist> {
        let checklists = self.checklists.read().await;
        checklists.get(key).cloned()
    }

    /// Update a checklist item
    pub async fn check_item(
        &self,
        checklist_key: &str,
        item_id: &str,
        checked_by: &str,
        notes: Option<&str>,
    ) -> Result<(), String> {
        let mut checklists = self.checklists.write().await;
        let checklist = checklists
            .get_mut(checklist_key)
            .ok_or("Checklist not found")?;

        let item = checklist.find_item_mut(item_id).ok_or("Item not found")?;

        item.check(checked_by.to_string(), notes.map(|s| s.to_string()));
        checklist.updated_at = chrono::Utc::now();

        Ok(())
    }

    /// Validate a checklist
    pub async fn validate_checklist(&self, key: &str) -> Option<ChecklistValidationResult> {
        let checklist = self.get_checklist(key).await?;

        let required_pending: Vec<String> = checklist
            .items
            .iter()
            .filter(|i| i.is_required && !i.is_checked)
            .map(|i| i.id.clone())
            .collect();

        let optional_pending = checklist
            .items
            .iter()
            .filter(|i| !i.is_required && !i.is_checked)
            .count();

        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        // Check for items that have evidence
        let items_with_evidence = checklist
            .items
            .iter()
            .filter(|i| i.is_checked && i.evidence.is_empty())
            .count();
        if items_with_evidence > 0 {
            warnings.push(format!(
                "{} checked items have no evidence attached",
                items_with_evidence
            ));
        }

        // Check documentation completeness
        let docs_items: Vec<_> = checklist
            .items
            .iter()
            .filter(|i| i.category == ChecklistCategory::Documentation && i.is_checked)
            .collect();
        if docs_items.is_empty() {
            recommendations.push("Consider documenting the migration decisions".to_string());
        }

        Some(ChecklistValidationResult {
            checklist_type: checklist.workflow_type.clone(),
            is_valid: required_pending.is_empty(),
            required_items_pending: required_pending,
            optional_items_pending: optional_pending,
            warnings,
            recommendations,
        })
    }

    /// Mark checklist as completed and move to completed list
    pub async fn complete_checklist(&self, key: &str) -> Result<WorkflowChecklist, String> {
        let validation = self
            .validate_checklist(key)
            .await
            .ok_or("Checklist not found")?;

        if !validation.is_valid {
            return Err(format!(
                "Cannot complete checklist: {} required items pending",
                validation.required_items_pending.len()
            ));
        }

        let mut checklists = self.checklists.write().await;
        let checklist = checklists.remove(key).ok_or("Checklist not found")?;

        let mut completed = self.completed_checklists.write().await;
        completed.push(checklist.clone());

        Ok(checklist)
    }

    /// Get all active checklists
    pub async fn get_active_checklists(&self) -> Vec<WorkflowChecklist> {
        let checklists = self.checklists.read().await;
        checklists.values().cloned().collect()
    }

    /// Get all completed checklists
    pub async fn get_completed_checklists(&self) -> Vec<WorkflowChecklist> {
        let completed = self.completed_checklists.read().await;
        completed.clone()
    }

    /// Generate a report of all checklists
    pub async fn generate_report(&self) -> ChecklistReport {
        let active = self.get_active_checklists().await;
        let completed = self.get_completed_checklists().await;

        let mut by_type: HashMap<String, (usize, usize)> = HashMap::new();

        for c in &active {
            let entry = by_type.entry(c.workflow_type.clone()).or_insert((0, 0));
            entry.0 += 1;
        }

        for c in &completed {
            let entry = by_type.entry(c.workflow_type.clone()).or_insert((0, 0));
            entry.1 += 1;
        }

        ChecklistReport {
            total_active: active.len(),
            total_completed: completed.len(),
            by_type,
            completed_checklists: completed,
        }
    }
}

/// Summary report of all checklists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistReport {
    pub total_active: usize,
    pub total_completed: usize,
    pub by_type: HashMap<String, (usize, usize)>,
    pub completed_checklists: Vec<WorkflowChecklist>,
}

/// Generator for creating checklists from templates
#[derive(Debug, Default)]
pub struct ChecklistTemplateGenerator;

impl ChecklistTemplateGenerator {
    /// Generate checklist for Provisioning workflow
    pub fn provisioning_template() -> WorkflowChecklist {
        let mut checklist = WorkflowChecklist::new("provisioning", "Provisioning Workflow");

        // Add specific items for provisioning
        checklist.items.push(ChecklistItem::new(
            "provisioning-specific-001".to_string(),
            ChecklistCategory::ActivityImplementation,
            "Worker infrastructure creation matches legacy provider selection".to_string(),
            true,
        ));

        checklist
    }

    /// Generate checklist for Execution workflow
    pub fn execution_template() -> WorkflowChecklist {
        let mut checklist = WorkflowChecklist::new("execution", "Execution Workflow");

        // Add specific items for execution
        checklist.items.push(ChecklistItem::new(
            "execution-specific-001".to_string(),
            ChecklistCategory::ActivityImplementation,
            "Job dispatch logic matches legacy behavior".to_string(),
            true,
        ));

        checklist
    }

    /// Generate checklist for Recovery workflow
    pub fn recovery_template() -> WorkflowChecklist {
        WorkflowChecklist::new("recovery", "Recovery Workflow")
    }

    /// Generate checklist for Cancellation workflow
    pub fn cancellation_template() -> WorkflowChecklist {
        WorkflowChecklist::new("cancellation", "Cancellation Workflow")
    }

    /// Generate checklist for Timeout workflow
    pub fn timeout_template() -> WorkflowChecklist {
        WorkflowChecklist::new("timeout", "Timeout Workflow")
    }

    /// Generate checklist for Cleanup workflow
    pub fn cleanup_template() -> WorkflowChecklist {
        WorkflowChecklist::new("cleanup", "Cleanup Workflow")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_checklist_creation() {
        let service = MigrationChecklistService::new();

        let key = service
            .create_checklist("provisioning", "Provisioning Workflow")
            .await;

        let checklist = service.get_checklist(&key).await.unwrap();

        assert_eq!(checklist.workflow_type, "provisioning");
        assert_eq!(checklist.workflow_name, "Provisioning Workflow");
        assert!(!checklist.items.is_empty());
    }

    #[tokio::test]
    async fn test_check_item() {
        let service = MigrationChecklistService::new();

        let key = service
            .create_checklist("execution", "Execution Workflow")
            .await;

        let result = service
            .check_item(
                &key,
                "execution-input-001",
                "developer@company.com",
                Some("Verified manually"),
            )
            .await;

        assert!(result.is_ok());

        let checklist = service.get_checklist(&key).await.unwrap();
        let item = checklist.find_item("execution-input-001").unwrap();

        assert!(item.is_checked);
        assert_eq!(
            item.checked_by.as_ref(),
            Some(&"developer@company.com".to_string())
        );
        assert!(item.notes.is_some());
    }

    #[tokio::test]
    async fn test_checklist_progress() {
        let service = MigrationChecklistService::new();

        let key = service
            .create_checklist("provisioning", "Provisioning Workflow")
            .await;

        let checklist = service.get_checklist(&key).await.unwrap();
        assert_eq!(checklist.progress(), 0.0);

        // Check first item
        service
            .check_item(&key, "provisioning-input-001", "dev@example.com", None)
            .await
            .unwrap();

        let checklist = service.get_checklist(&key).await.unwrap();
        let total = checklist.items.len();
        assert!((checklist.progress() - (100.0 / total as f64)).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_validation_all_required() {
        let service = MigrationChecklistService::new();

        let key = service
            .create_checklist("execution", "Execution Workflow")
            .await;

        let checklist = service.get_checklist(&key).await.unwrap();
        let optional_count = checklist.items.iter().filter(|i| !i.is_required).count();

        // Check all required items
        for item in checklist.items.iter().filter(|i| i.is_required) {
            service
                .check_item(&key, &item.id, "dev@example.com", None)
                .await
                .unwrap();
        }

        let validation = service.validate_checklist(&key).await.unwrap();

        assert!(validation.is_valid);
        assert!(validation.required_items_pending.is_empty());
        assert_eq!(validation.optional_items_pending, optional_count);
    }

    #[tokio::test]
    async fn test_complete_checklist() {
        let service = MigrationChecklistService::new();

        let key = service
            .create_checklist("provisioning", "Provisioning Workflow")
            .await;

        // Try to complete without checking items
        let result = service.complete_checklist(&key).await;
        assert!(result.is_err());

        // Check all items
        let checklist = service.get_checklist(&key).await.unwrap();
        for item in checklist.items.iter() {
            service
                .check_item(&key, &item.id, "dev@example.com", None)
                .await
                .unwrap();
        }

        // Now should succeed
        let result = service.complete_checklist(&key).await;
        assert!(result.is_ok());

        // Should be in completed list
        let completed = service.get_completed_checklists().await;
        assert_eq!(completed.len(), 1);

        // Should not be in active list
        let active = service.get_active_checklists().await;
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_template_generator() {
        let provisioning = ChecklistTemplateGenerator::provisioning_template();
        let execution = ChecklistTemplateGenerator::execution_template();

        assert_eq!(provisioning.workflow_type, "provisioning");
        assert_eq!(execution.workflow_type, "execution");

        // Templates should have additional specific items
        let provisioning_specific = provisioning
            .items
            .iter()
            .any(|i| i.id.contains("provisioning-specific"));
        assert!(provisioning_specific);
    }
}
