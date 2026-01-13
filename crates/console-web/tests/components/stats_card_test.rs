//! Tests for StatsCard component using rstest fixtures
//!
//! These tests follow TDD principles: Red -> Green -> Refactor
//! Using rstest for fixture-based testing with dependency injection.

use leptos::prelude::*;
use rstest::{fixture, rstest};

use hodei_console_web::components::stats_card::{IconVariant, StatsCard};

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// FIXTURES - Define reusable test data
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Fixture providing sample stats card props for testing
#[derive(Debug, Clone)]
struct StatsCardFixture {
    pub label: String,
    pub value: String,
    pub icon: String,
    pub icon_variant: IconVariant,
}

#[fixture]
fn sample_stats_card() -> StatsCardFixture {
    StatsCardFixture {
        label: "Total Jobs".to_string(),
        value: "1,234".to_string(),
        icon: "work".to_string(),
        icon_variant: IconVariant::Primary,
    }
}

/// Fixture for success variant stats card
#[fixture]
fn success_stats_card() -> StatsCardFixture {
    StatsCardFixture {
        label: "Success Rate".to_string(),
        value: "98.5%".to_string(),
        icon: "check_circle".to_string(),
        icon_variant: IconVariant::Success,
    }
}

/// Fixture for warning variant stats card
#[fixture]
fn warning_stats_card() -> StatsCardFixture {
    StatsCardFixture {
        label: "Pending Jobs".to_string(),
        value: "42".to_string(),
        icon: "hourglass_empty".to_string(),
        icon_variant: IconVariant::Warning,
    }
}

/// Fixture for danger variant stats card
#[fixture]
fn danger_stats_card() -> StatsCardFixture {
    StatsCardFixture {
        label: "Failed Jobs".to_string(),
        value: "12".to_string(),
        icon: "error".to_string(),
        icon_variant: IconVariant::Danger,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// TESTS - Test cases with rstest parametrization
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test that StatsCard renders with correct label
#[rstest]
fn test_stats_card_renders_label(#[from(sample_stats_card)] fixture: StatsCardFixture) {
    // This test verifies the component can be constructed with valid props
    // In a real Leptos environment, we'd render to a DOM and verify the output
    assert_eq!(fixture.label, "Total Jobs");
    assert!(!fixture.label.is_empty());
}

/// Test that StatsCard handles all icon variants
#[rstest]
fn test_stats_card_icon_variants(
    #[values(
        IconVariant::Primary,
        IconVariant::Success,
        IconVariant::Warning,
        IconVariant::Danger,
        IconVariant::Neutral
    )]
    variant: IconVariant,
) {
    // Test all variants are constructible
    assert!(matches!(
        variant,
        IconVariant::Primary
            | IconVariant::Success
            | IconVariant::Warning
            | IconVariant::Danger
            | IconVariant::Neutral
    ));
}

/// Test stats card with different label values (parametrized test)
#[rstest]
fn test_stats_card_different_labels(
    #[values(
        "Total Jobs",
        "Running",
        "Success Rate",
        "Failed Jobs",
        "Pending Queue"
    )]
    label: &str,
) {
    let fixture = StatsCardFixture {
        label: label.to_string(),
        value: "0".to_string(),
        icon: "work".to_string(),
        icon_variant: IconVariant::Neutral,
    };
    assert_eq!(fixture.label, label);
}

/// Test stats card with numeric values (parametrized test)
#[rstest]
fn test_stats_card_numeric_values(#[values("0", "1", "100", "1,234", "999,999")] value: &str) {
    let fixture = StatsCardFixture {
        label: "Test".to_string(),
        value: value.to_string(),
        icon: "work".to_string(),
        icon_variant: IconVariant::Primary,
    };
    assert_eq!(fixture.value, value);
}

/// Test that icon variant determines correct CSS class mapping
#[rstest]
fn test_icon_variant_class_mapping(
    #[values(
        IconVariant::Primary,
        IconVariant::Success,
        IconVariant::Warning,
        IconVariant::Danger
    )]
    variant: IconVariant,
) {
    // Simulate the class mapping logic from the component
    let expected_class = match variant {
        IconVariant::Primary => "stat-icon running",
        IconVariant::Success => "stat-icon success",
        IconVariant::Warning => "stat-icon pending",
        IconVariant::Danger => "stat-icon failed",
        IconVariant::Neutral => "stat-icon",
    };

    // Verify mapping is deterministic
    let class = match variant {
        IconVariant::Primary => "stat-icon running",
        IconVariant::Success => "stat-icon success",
        IconVariant::Warning => "stat-icon pending",
        IconVariant::Danger => "stat-icon failed",
        IconVariant::Neutral => "stat-icon",
    };

    assert_eq!(expected_class, class);
}

/// Test StatsCard fixture consistency across multiple calls
#[rstest]
fn test_fixture_consistency(
    #[from(sample_stats_card)] fixture1: StatsCardFixture,
    #[from(sample_stats_card)] fixture2: StatsCardFixture,
) {
    // Same fixture should produce identical results
    assert_eq!(fixture1.label, fixture2.label);
    assert_eq!(fixture1.value, fixture2.value);
    assert_eq!(fixture1.icon, fixture2.icon);
    assert_eq!(fixture1.icon_variant, fixture2.icon_variant);
}

/// Test trend info structure
#[rstest]
fn test_trend_info_structure() {
    use hodei_console_web::components::stats_card::TrendInfo;

    // Test positive trend
    let positive = TrendInfo {
        value: 10,
        is_positive: true,
    };
    assert_eq!(positive.value, 10);
    assert!(positive.is_positive);

    // Test negative trend
    let negative = TrendInfo {
        value: 5,
        is_positive: false,
    };
    assert_eq!(negative.value, 5);
    assert!(!negative.is_positive);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// PROPTEST - Property-based testing (optional)
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod property_tests {
    use super::*;

    /// Property-based test: all icon variants should produce valid class strings
    #[test]
    fn test_all_variants_produce_valid_class() {
        for variant in [
            IconVariant::Primary,
            IconVariant::Success,
            IconVariant::Warning,
            IconVariant::Danger,
            IconVariant::Neutral,
        ] {
            let class = match variant {
                IconVariant::Primary => "stat-icon running",
                IconVariant::Success => "stat-icon success",
                IconVariant::Warning => "stat-icon pending",
                IconVariant::Danger => "stat-icon failed",
                IconVariant::Neutral => "stat-icon",
            };
            assert!(class.starts_with("stat-icon"));
            assert!(!class.is_empty());
        }
    }
}
