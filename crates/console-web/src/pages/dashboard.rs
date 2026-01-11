//! Dashboard page - Overview of system status

use crate::components::{IconVariant, StatsCard, StatusBadge};
use leptos::prelude::*;

#[component]
pub fn Dashboard() -> impl IntoView {
    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Dashboard"</h1>
                    <p class="page-subtitle">"Real-time overview of your Hodei Jobs Platform"</p>
                </div>
                <div class="quick-actions">
                    <button class="btn btn-primary">
                        <span class="material-symbols-outlined">"add"</span>
                        "New Job"
                    </button>
                </div>
            </div>

            <div class="stats-grid">
                <StatsCard label="Total Jobs".to_string() value="1247".to_string() icon="work".to_string() icon_variant=IconVariant::Primary />
                <StatsCard label="Running".to_string() value="12".to_string() icon="sync".to_string() icon_variant=IconVariant::Primary />
                <StatsCard label="Success (7d)".to_string() value="1198".to_string() icon="check_circle".to_string() icon_variant=IconVariant::Success />
                <StatsCard label="Failed (7d)".to_string() value="37".to_string() icon="error".to_string() icon_variant=IconVariant::Danger />
            </div>

            <div class="card">
                <div class="card-header">
                    <h3 class="card-title">"Recent Jobs"</h3>
                </div>
                <div class="card-body" style="padding: 0;">
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Status"</th>
                                <th>"Job ID"</th>
                                <th>"Name"</th>
                                <th>"Duration"</th>
                                <th>"Started"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td><StatusBadge status=crate::components::JobStatusBadge::Success label="job-1247".to_string() animate=false /></td>
                                <td class="monospace">"job-1247"</td>
                                <td>"data-processing-pipeline"</td>
                                <td class="monospace">"2m 34s"</td>
                                <td>"5 min ago"</td>
                            </tr>
                            <tr>
                                <td><StatusBadge status=crate::components::JobStatusBadge::Running label="job-1246".to_string() animate=true /></td>
                                <td class="monospace">"job-1246"</td>
                                <td>"ml-model-training"</td>
                                <td class="monospace">"14m 22s"</td>
                                <td>"12 min ago"</td>
                            </tr>
                            <tr>
                                <td><StatusBadge status=crate::components::JobStatusBadge::Success label="job-1245".to_string() animate=false /></td>
                                <td class="monospace">"job-1245"</td>
                                <td>"image-processing"</td>
                                <td class="monospace">"45s"</td>
                                <td>"1h ago"</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    }
}
