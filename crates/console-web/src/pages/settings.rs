//! Settings page - Platform configuration
//!
//! Provides platform-wide settings including general configuration,
//! provider overview, notifications, and security settings.

use leptos::prelude::*;

/// Settings page component
#[component]
pub fn Settings() -> impl IntoView {
    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Settings"</h1>
                    <p class="page-subtitle">"Configure and manage platform settings"</p>
                </div>
            </div>

            <div style="display: grid; grid-template-columns: 240px 1fr; gap: 2rem;">
                // Settings Navigation
                <aside class="settings-nav">
                    <nav style="position: sticky; top: 1rem;">
                        <a href="#general" class="settings-nav-item active">
                            <span class="material-symbols-outlined" style="font-size: 1.25rem;">"tune"</span>
                            "General"
                        </a>
                        <a href="#providers" class="settings-nav-item">
                            <span class="material-symbols-outlined" style="font-size: 1.25rem;">"cloud"</span>
                            "Providers"
                        </a>
                        <a href="#notifications" class="settings-nav-item">
                            <span class="material-symbols-outlined" style="font-size: 1.25rem;">"notifications"</span>
                            "Notifications"
                        </a>
                        <a href="#security" class="settings-nav-item">
                            <span class="material-symbols-outlined" style="font-size: 1.25rem;">"security"</span>
                            "Security"
                        </a>
                        <a href="#users" class="settings-nav-item">
                            <span class="material-symbols-outlined" style="font-size: 1.25rem;">"people"</span>
                            "Users"
                        </a>
                    </nav>
                </aside>

                // Settings Content
                <div style="display: flex; flex-direction: column; gap: 2rem;">
                    // General Settings
                    <section id="general" class="card">
                        <div class="card-header">
                            <h3 class="card-title">"General Settings"</h3>
                        </div>
                        <div class="card-body">
                            <form class="settings-form">
                                <div class="form-row">
                                    <div class="form-group">
                                        <label class="form-label">"Platform Name"</label>
                                        <input type="text" class="form-input" value="Hodei Production Cluster" />
                                    </div>
                                    <div class="form-group">
                                        <label class="form-label">"Environment"</label>
                                        <select class="form-select">
                                            <option value="production">"Production"</option>
                                            <option value="staging">"Staging"</option>
                                            <option value="development">"Development"</option>
                                        </select>
                                    </div>
                                </div>

                                <div class="form-row">
                                    <div class="form-group">
                                        <label class="form-label">"Default Timeout (minutes)"</label>
                                        <input type="number" class="form-input" value="60" />
                                    </div>
                                    <div class="form-group">
                                        <label class="form-label">"Max Concurrent Jobs"</label>
                                        <input type="number" class="form-input" value="25" />
                                    </div>
                                </div>

                                <div class="form-group">
                                    <label class="form-label">"Default Namespace"</label>
                                    <input type="text" class="form-input" value="hodei-system" />
                                    <span class="form-hint">"Namespace where jobs and workers will be created by default"</span>
                                </div>

                                <div class="form-actions">
                                    <button type="button" class="btn btn-primary">
                                        <span class="material-symbols-outlined" style="font-size: 1rem;">"save"</span>
                                        "Save Changes"
                                    </button>
                                </div>
                            </form>
                        </div>
                    </section>

                    // Providers Overview
                    <section id="providers" class="card">
                        <div class="card-header">
                            <h3 class="card-title">"Providers Overview"</h3>
                            <a href="/providers" class="btn btn-secondary btn-sm">
                                "Manage Providers"
                            </a>
                        </div>
                        <div class="card-body">
                            <div class="stats-grid" style="grid-template-columns: repeat(3, 1fr);">
                                <div class="stat-card">
                                    <p class="stat-label">"Active Providers"</p>
                                    <p class="stat-value">"3"</p>
                                </div>
                                <div class="stat-card">
                                    <p class="stat-label">"Total Workers"</p>
                                    <p class="stat-value">"24"</p>
                                </div>
                                <div class="stat-card">
                                    <p class="stat-label">"Health Score"</p>
                                    <p class="stat-value" style="color: var(--color-success);">"98%"</p>
                                </div>
                            </div>
                        </div>
                    </section>

                    // Notifications
                    <section id="notifications" class="card">
                        <div class="card-header">
                            <h3 class="card-title">"Notifications"</h3>
                        </div>
                        <div class="card-body">
                            <div style="display: flex; flex-direction: column; gap: 1rem;">
                                <NotificationSetting
                                    name="Email (SMTP)"
                                    description="Receive alerts and job status updates via email"
                                    enabled=true
                                    config_url="/settings/notifications/email"
                                />
                                <NotificationSetting
                                    name="Slack"
                                    description="Post job status and alerts to Slack channels"
                                    enabled=true
                                    config_url="/settings/notifications/slack"
                                />
                                <NotificationSetting
                                    name="Discord"
                                    description="Receive notifications in Discord webhooks"
                                    enabled=false
                                    config_url="/settings/notifications/discord"
                                />
                                <NotificationSetting
                                    name="PagerDuty"
                                    description="Escalate critical issues to on-call teams"
                                    enabled=false
                                    config_url="/settings/notifications/pagerduty"
                                />
                            </div>
                        </div>
                    </section>

                    // Security Settings
                    <section id="security" class="card">
                        <div class="card-header">
                            <h3 class="card-title">"Security Settings"</h3>
                        </div>
                        <div class="card-body">
                            <form class="settings-form">
                                <div class="form-group">
                                    <div style="display: flex; align-items: center; justify-content: space-between;">
                                        <div>
                                            <label class="form-label" style="margin: 0;">"Enable RBAC"</label>
                                            <p style="font-size: 0.875rem; color: var(--text-secondary); margin: 0.25rem 0 0;">
                                                "Enforce strict role-based access control permissions"
                                            </p>
                                        </div>
                                        <label class="toggle">
                                            <input type="checkbox" checked />
                                            <span class="toggle-slider"></span>
                                        </label>
                                    </div>
                                </div>

                                <div class="form-group">
                                    <label class="form-label">"Audit Log Retention"</label>
                                    <select class="form-select">
                                        <option value="7">"7 days"</option>
                                        <option value="30">"30 days"</option>
                                        <option value="90">"90 days"</option>
                                        <option value="365" selected>"1 year"</option>
                                    </select>
                                </div>

                                <div class="form-group">
                                    <div style="display: flex; align-items: center; justify-content: space-between;">
                                        <div>
                                            <label class="form-label" style="margin: 0;">"mTLS Authentication"</label>
                                            <p style="font-size: 0.875rem; color: var(--text-secondary); margin: 0.25rem 0 0;">
                                                "Require mutual TLS for all gRPC connections"
                                            </p>
                                        </div>
                                        <label class="toggle">
                                            <input type="checkbox" />
                                            <span class="toggle-slider"></span>
                                        </label>
                                    </div>
                                </div>

                                <div class="form-group">
                                    <div style="display: flex; align-items: center; justify-content: space-between;">
                                        <div>
                                            <label class="form-label" style="margin: 0;">"OTP Required for Worker Registration"</label>
                                            <p style="font-size: 0.875rem; color: var(--text-secondary); margin: 0.25rem 0 0;">
                                                "Workers must provide a valid OTP to register"
                                            </p>
                                        </div>
                                        <label class="toggle">
                                            <input type="checkbox" checked />
                                            <span class="toggle-slider"></span>
                                        </label>
                                    </div>
                                </div>

                                <div class="form-actions">
                                    <button type="button" class="btn btn-primary">
                                        <span class="material-symbols-outlined" style="font-size: 1rem;">"save"</span>
                                        "Save Security Settings"
                                    </button>
                                </div>
                            </form>
                        </div>
                    </section>
                </div>
            </div>
        </div>
    }
}

/// Notification Setting Component
#[component]
fn NotificationSetting(
    name: &'static str,
    description: &'static str,
    enabled: bool,
    config_url: &'static str,
) -> impl IntoView {
    view! {
        <div style="display: flex; align-items: center; justify-content: space-between; padding: 1rem; background: var(--bg-tertiary); border-radius: var(--radius-lg);">
            <div style="flex: 1;">
                <h4 style="font-weight: 600; margin: 0;">{name}</h4>
                <p style="font-size: 0.875rem; color: var(--text-secondary); margin: 0.25rem 0 0;">{description}</p>
            </div>
            <div style="display: flex; align-items: center; gap: 0.75rem;">
                <span class=format!("badge {}", if enabled { "badge-success" } else { "badge-neutral" })>
                    {if enabled { "Enabled" } else { "Disabled" }}
                </span>
                <a href=config_url class="btn btn-secondary btn-sm">
                    <span class="material-symbols-outlined" style="font-size: 1rem;">"edit"</span>
                    "Configure"
                </a>
            </div>
        </div>
    }
}
