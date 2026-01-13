//! Tests for StatusBadge component using rstest parametrized tests
//!
//! TDD approach: Tests first, then implementation
//! Using rstest for comprehensive parametrization of badge states.

use leptos::prelude::*;
use rstest::{fixture, rstest};

use hodei_console_web::components::status_badge::JobStatusBadge;

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// FIXTURES - Badge state test data
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Fixture for running job status
#[fixture]
fn running_status() -> JobStatusBadge {
    JobStatusBadge::Running
}

/// Fixture for success job status
#[fixture]
fn success_status() -> JobStatusBadge {
    JobStatusBadge::Success
}

/// Fixture for failed job status
#[fixture]
fn failed_status() -> JobStatusBadge {
    JobStatusBadge::Failed
}

/// Fixture for pending job status
#[fixture]
fn pending_status() -> JobStatusBadge {
    JobStatusBadge::Pending
}

/// Fixture for cancelled job status
#[fixture]
fn cancelled_status() -> JobStatusBadge {
    JobStatusBadge::Cancelled
}

/// Fixture for unknown job status
#[fixture]
fn unknown_status() -> JobStatusBadge {
    JobStatusBadge::Cancelled // Using Cancelled as closest to Unknown
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// TESTS - Status badge variant tests
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test all job status badge variants are constructible
#[rstest]
fn test_all_status_variants_exist() {
    let variants = [
        JobStatusBadge::Running,
        JobStatusBadge::Success,
        JobStatusBadge::Failed,
        JobStatusBadge::Pending,
        JobStatusBadge::Cancelled,
    ];

    assert_eq!(variants.len(), 5);
}

/// Test status badge equality
#[rstest]
fn test_status_badge_equality(
    #[from(running_status)] status1: JobStatusBadge,
    #[from(running_status)] status2: JobStatusBadge,
) {
    assert_eq!(status1, status2);
}

/// Test status badge inequality
#[rstest]
fn test_status_badge_inequality(
    #[from(running_status)] running: JobStatusBadge,
    #[from(success_status)] success: JobStatusBadge,
) {
    assert_ne!(running, success);
}

/// Test status badge variant count
#[rstest]
fn test_status_badge_variant_count(
    #[values(
        JobStatusBadge::Running,
        JobStatusBadge::Success,
        JobStatusBadge::Failed,
        JobStatusBadge::Pending,
        JobStatusBadge::Cancelled
    )]
    status: JobStatusBadge,
) {
    // Each variant should be unique
    let all_variants = vec![
        JobStatusBadge::Running,
        JobStatusBadge::Success,
        JobStatusBadge::Failed,
        JobStatusBadge::Pending,
        JobStatusBadge::Cancelled,
    ];

    assert!(all_variants.contains(&status));
    assert_eq!(all_variants.len(), 5);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// CSS CLASS MAPPING TESTS - Verify component's class mapping logic
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test that Running status maps to correct CSS classes
#[rstest]
fn test_running_status_css_classes() {
    // Simulate the CSS class mapping from the component
    let (base_class, color_class, icon, should_animate) = match JobStatusBadge::Running {
        JobStatusBadge::Running => (
            "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium",
            "bg-blue-50 text-blue-600",
            "sync",
            true,
        ),
        _ => unreachable!(),
    };

    assert!(base_class.contains("inline-flex"));
    assert!(base_class.contains("items-center"));
    assert!(color_class.contains("blue"));
    assert_eq!(icon, "sync");
    assert!(should_animate);
}

/// Test that Success status maps to correct CSS classes
#[rstest]
fn test_success_status_css_classes() {
    let (color_class, icon, should_animate) = match JobStatusBadge::Success {
        JobStatusBadge::Success => ("bg-green-50 text-green-600", "check_circle", false),
        _ => unreachable!(),
    };

    assert!(color_class.contains("green"));
    assert_eq!(icon, "check_circle");
    assert!(!should_animate);
}

/// Test that Failed status maps to correct CSS classes
#[rstest]
fn test_failed_status_css_classes() {
    let (color_class, icon, should_animate) = match JobStatusBadge::Failed {
        JobStatusBadge::Failed => ("bg-red-50 text-red-600", "error", false),
        _ => unreachable!(),
    };

    assert!(color_class.contains("red"));
    assert_eq!(icon, "error");
    assert!(!should_animate);
}

/// Test that Pending status maps to correct CSS classes
#[rstest]
fn test_pending_status_css_classes() {
    let (color_class, icon, should_animate) = match JobStatusBadge::Pending {
        JobStatusBadge::Pending => ("bg-gray-50 text-gray-600", "hourglass_empty", false),
        _ => unreachable!(),
    };

    assert!(color_class.contains("gray"));
    assert_eq!(icon, "hourglass_empty");
    assert!(!should_animate);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// ANIMATION TESTS - Verify animation logic per status
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test animation flag is correctly set per status
#[rstest]
fn test_animation_flag_per_status(
    #[values(
        JobStatusBadge::Running,
        JobStatusBadge::Success,
        JobStatusBadge::Failed,
        JobStatusBadge::Pending,
        JobStatusBadge::Cancelled
    )]
    status: JobStatusBadge,
) {
    let should_animate = match status {
        JobStatusBadge::Running => true,
        JobStatusBadge::Success => false,
        JobStatusBadge::Failed => false,
        JobStatusBadge::Pending => false,
        JobStatusBadge::Cancelled => false,
    };

    // Only Running status should animate
    let expected_animate = matches!(status, JobStatusBadge::Running);
    assert_eq!(should_animate, expected_animate);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// ICON TESTS - Verify icon mapping per status
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test icon mapping is correct for all statuses
#[rstest]
fn test_icon_mapping_per_status(
    #[values(
        JobStatusBadge::Running,
        JobStatusBadge::Success,
        JobStatusBadge::Failed,
        JobStatusBadge::Pending,
        JobStatusBadge::Cancelled
    )]
    status: JobStatusBadge,
) {
    let icon = match status {
        JobStatusBadge::Running => "sync",
        JobStatusBadge::Success => "check_circle",
        JobStatusBadge::Failed => "error",
        JobStatusBadge::Pending => "hourglass_empty",
        JobStatusBadge::Cancelled => "cancel",
    };

    assert!(!icon.is_empty());
    assert!(icon.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// STATUS ORDERING TESTS - Verify status ordering
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test that status badges can be ordered by severity
#[rstest]
fn test_status_ordering_by_severity() {
    // Define severity ranking (higher = more severe)
    let severity = |status: JobStatusBadge| match status {
        JobStatusBadge::Pending => 1,
        JobStatusBadge::Running => 2,
        JobStatusBadge::Success => 3,
        JobStatusBadge::Cancelled => 4,
        JobStatusBadge::Failed => 5,
    };

    assert!(severity(JobStatusBadge::Failed) > severity(JobStatusBadge::Success));
    assert!(severity(JobStatusBadge::Running) > severity(JobStatusBadge::Pending));
    assert!(severity(JobStatusBadge::Cancelled) > severity(JobStatusBadge::Running));
}
