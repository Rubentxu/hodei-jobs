//! Stats Card Component
//!
//! A reusable statistics card showing a metric.

use leptos::prelude::*;

/// Icon color variants
#[derive(Clone, Debug, PartialEq)]
pub enum IconVariant {
    Primary,
    Success,
    Warning,
    Danger,
    Neutral,
}

/// Stats Card component
#[component]
pub fn StatsCard(
    /// Label describing the metric
    label: String,
    /// Numeric value to display
    value: String,
    /// Icon name (Material Symbols)
    icon: String,
    /// Icon color variant
    icon_variant: IconVariant,
) -> impl IntoView {
    let icon_class = match icon_variant {
        IconVariant::Primary => "stat-icon running",
        IconVariant::Success => "stat-icon success",
        IconVariant::Warning => "stat-icon pending",
        IconVariant::Danger => "stat-icon failed",
        IconVariant::Neutral => "stat-icon",
    };

    view! {
        <div class="stat-card">
            <div class="stat-card-header">
                <div class=icon_class>
                    <span class="material-symbols-outlined">{icon}</span>
                </div>
            </div>
            <p class="stat-label">{label}</p>
            <p class="stat-value">{value}</p>
        </div>
    }
}

/// Trend information
#[derive(Clone, Debug, PartialEq)]
pub struct TrendInfo {
    pub value: i32,
    pub is_positive: bool,
}
