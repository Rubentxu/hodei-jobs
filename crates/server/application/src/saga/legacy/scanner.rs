//!
//! # Legacy Code Scanner
//!
//! Scans the codebase for legacy saga patterns and dependencies that need
//! to be migrated to the new saga-engine v4.0.
//!

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during scanning
#[derive(Debug, Error, Clone)]
#[error("{message}")]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

impl ScanError {
    pub fn from_io_error(path: PathBuf, source: std::io::Error) -> Self {
        Self {
            path,
            message: source.to_string(),
        }
    }
}

/// Target types for scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanTarget {
    /// Scan for direct saga orchestrator usage
    SagaOrchestrator,
    /// Scan for legacy command handlers
    CommandHandlers,
    /// Scan for domain events
    DomainEvents,
    /// Scan for repository patterns
    Repositories,
    /// Scan for all legacy patterns
    All,
}

impl ScanTarget {
    /// Returns the patterns to search for
    pub fn patterns(&self) -> Vec<&'static str> {
        match self {
            ScanTarget::SagaOrchestrator => {
                vec!["InMemorySagaOrchestrator", "PostgresSagaOrchestrator"]
            }
            ScanTarget::CommandHandlers => vec!["impl.*CommandHandler", "trait.*CommandHandler"],
            ScanTarget::DomainEvents => vec!["pub enum.*Event", "struct.*Event"],
            ScanTarget::Repositories => vec!["trait.*Repository", "impl.*Repository"],
            ScanTarget::All => vec!["InMemorySagaOrchestrator", "CommandHandler", "Repository"],
        }
    }

    /// Returns file extensions to scan
    pub fn extensions(&self) -> Vec<&'static str> {
        vec!["rs"]
    }
}

/// Result of a single file scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileScanResult {
    pub file_path: PathBuf,
    pub matches: Vec<Match>,
    pub is_legacy: bool,
}

impl FileScanResult {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            matches: vec![],
            is_legacy: false,
        }
    }

    pub fn with_match(mut self, matched: Match) -> Self {
        self.matches.push(matched);
        self.is_legacy = true;
        self
    }
}

/// A single pattern match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub line_number: usize,
    pub line_content: String,
    pub pattern: String,
    pub context: String,
}

/// Overall scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub target: ScanTarget,
    pub scanned_files: usize,
    pub files_with_legacy: usize,
    pub total_matches: usize,
    pub file_results: Vec<FileScanResult>,
    pub scanned_at: chrono::DateTime<chrono::Utc>,
}

impl ScanResult {
    pub fn new(target: ScanTarget) -> Self {
        Self {
            target,
            scanned_files: 0,
            files_with_legacy: 0,
            total_matches: 0,
            file_results: vec![],
            scanned_at: chrono::Utc::now(),
        }
    }
}

/// Legacy code scanner
#[derive(Debug)]
pub struct LegacyCodeScanner {
    root_path: PathBuf,
    exclude_patterns: Vec<String>,
}

impl Default for LegacyCodeScanner {
    fn default() -> Self {
        Self::new(PathBuf::from("."))
    }
}

impl LegacyCodeScanner {
    /// Creates a new scanner
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            root_path,
            exclude_patterns: vec![
                "target/".to_string(),
                ".git/".to_string(),
                "*.swp".to_string(),
            ],
        }
    }

    /// Adds an exclude pattern
    pub fn add_exclude_pattern(mut self, pattern: String) -> Self {
        self.exclude_patterns.push(pattern);
        self
    }

    /// Scans for a specific target
    pub fn scan(&self, target: ScanTarget) -> Result<ScanResult, ScanError> {
        let patterns = target.patterns();
        let extensions = target.extensions();

        let mut result = ScanResult::new(target);

        // Walk the directory tree
        self.walk_directory(&self.root_path, &extensions, &patterns, &mut result)?;

        Ok(result)
    }

    fn walk_directory(
        &self,
        dir: &PathBuf,
        extensions: &[&str],
        patterns: &[&str],
        result: &mut ScanResult,
    ) -> Result<(), ScanError> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if self.should_exclude(&path) {
                    continue;
                }

                if path.is_dir() {
                    self.walk_directory(&path, extensions, patterns, result)?;
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if extensions.contains(&ext) {
                            result.scanned_files += 1;
                            if let Ok(file_result) = self.scan_file(&path, patterns) {
                                result.file_results.push(file_result.clone());
                                if file_result.is_legacy {
                                    result.files_with_legacy += 1;
                                    result.total_matches += file_result.matches.len();
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn should_exclude(&self, path: &PathBuf) -> bool {
        let path_str = path.to_string_lossy();
        self.exclude_patterns
            .iter()
            .any(|pattern| path_str.contains(pattern))
    }

    fn scan_file(&self, path: &PathBuf, patterns: &[&str]) -> Result<FileScanResult, ScanError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ScanError::from_io_error(path.clone(), e))?;

        let mut file_result = FileScanResult::new(path.clone());

        for (line_num, line) in content.lines().enumerate() {
            for pattern in patterns {
                if contains_pattern(line, pattern) {
                    let context = get_context(&content, line_num, 2);
                    file_result.matches.push(Match {
                        line_number: line_num + 1,
                        line_content: line.to_string(),
                        pattern: pattern.to_string(),
                        context,
                    });
                    file_result.is_legacy = true;
                }
            }
        }

        Ok(file_result)
    }
}

fn contains_pattern(text: &str, pattern: &str) -> bool {
    // Simple pattern matching (no regex for now)
    // Handle common patterns with basic string contains
    if pattern.contains(".*") {
        // Convert regex-like pattern to simple contains
        let parts: Vec<&str> = pattern.split(".*").collect();
        if parts.len() == 2 {
            return text.contains(parts[0]) && text.contains(parts[1]);
        }
    }
    text.contains(pattern)
}

fn get_context(content: &str, center: usize, context_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = center.saturating_sub(context_lines);
    let end = (center + context_lines + 1).min(lines.len());

    lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:4}: {}", start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Migration effort estimator based on scan results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationEffortEstimate {
    pub total_files_to_migrate: usize,
    pub total_matches: usize,
    pub estimated_hours: f64,
    pub risk_level: RiskLevel,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl From<&ScanResult> for MigrationEffortEstimate {
    fn from(scan: &ScanResult) -> Self {
        let total_matches = scan.total_matches;
        let files = scan.files_with_legacy;

        // Rough estimate: 1 hour per match, with complexity factors
        let base_hours = total_matches as f64 * 1.5;
        let complexity_factor = if files > 20 { 1.5 } else { 1.0 };

        let estimated_hours = base_hours * complexity_factor;

        let risk_level = if files > 50 || total_matches > 200 {
            RiskLevel::Critical
        } else if files > 20 || total_matches > 100 {
            RiskLevel::High
        } else if files > 10 || total_matches > 50 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        let mut recommendations = Vec::new();
        if files > 0 {
            recommendations.push(format!(
                "Start by migrating {} files with {} total legacy patterns",
                files, total_matches
            ));
        }
        if matches!(risk_level, RiskLevel::High | RiskLevel::Critical) {
            recommendations.push("Consider a phased migration approach to reduce risk".to_string());
            recommendations.push("Implement dual-write support during transition".to_string());
        }

        Self {
            total_files_to_migrate: files,
            total_matches,
            estimated_hours,
            risk_level,
            recommendations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_target_patterns() {
        let all = ScanTarget::All.patterns();
        assert!(!all.is_empty());
    }

    #[test]
    fn test_file_scan_result() {
        let path = PathBuf::from("test.rs");
        let result = FileScanResult::new(path.clone());
        assert_eq!(result.file_path, path);
        assert!(!result.is_legacy);
    }

    #[test]
    fn test_migration_effort_estimate() {
        let scan_result = ScanResult::new(ScanTarget::All);
        let estimate = MigrationEffortEstimate::from(&scan_result);

        assert_eq!(estimate.total_files_to_migrate, 0);
        assert_eq!(estimate.total_matches, 0);
        assert!(matches!(estimate.risk_level, RiskLevel::Low));
    }

    #[test]
    fn test_contains_pattern() {
        assert!(contains_pattern("struct MyOrchestrator {}", "Orchestrator"));
        assert!(!contains_pattern("struct MyStruct {}", "Orchestrator"));
    }

    #[test]
    fn test_contains_pattern_with_regex_like() {
        assert!(contains_pattern(
            "impl MyHandler implements CommandHandler for MyType {}",
            "impl.*CommandHandler"
        ));
    }
}
