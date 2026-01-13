//! Status Badge Component
//!
//! Displays job/worker status with appropriate colors and icons.

use leptos::prelude::*;

/// Job status variants
#[derive(Clone, Debug, PartialEq)]
pub enum JobStatusBadge {
    Running,
    Success,
    Failed,
    Pending,
    Cancelled,
}

/// Status Badge component
#[component]
pub fn StatusBadge(
    /// Status variant
    status: JobStatusBadge,
    /// Label text
    label: String,
    /// Whether to animate
    animate: bool,
) -> impl IntoView {
    let (color_class, icon) = match status {
        JobStatusBadge::Running => ("running", "sync"),
        JobStatusBadge::Success => ("success", "check_circle"),
        JobStatusBadge::Failed => ("failed", "error"),
        JobStatusBadge::Pending => ("pending", "hourglass_empty"),
        JobStatusBadge::Cancelled => ("pending", "cancel"),
    };

    let animate_class = if animate { "animate-pulse" } else { "" };

    view! {
        <span class=format!("status-badge {}", color_class)>
            <span class=format!("dot {}", animate_class)></span>
            <span class="material-symbols-outlined" style="font-size: 0.875rem;">{icon}</span>
            {label}
        </span>
    }
}
