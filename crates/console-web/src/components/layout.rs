//! Layout components - Sidebar and main layout wrapper

use leptos::prelude::*;

/// Navigation item type
#[derive(Clone, Debug, PartialEq)]
pub struct NavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub icon: &'static str,
}

pub static NAV_ITEMS: &[NavItem] = &[
    NavItem {
        label: "Dashboard",
        href: "/",
        icon: "dashboard",
    },
    NavItem {
        label: "Jobs",
        href: "/jobs",
        icon: "work",
    },
    NavItem {
        label: "Workers",
        href: "/workers",
        icon: "memory",
    },
    NavItem {
        label: "Scheduler",
        href: "/scheduler",
        icon: "schedule",
    },
    NavItem {
        label: "Providers",
        href: "/providers",
        icon: "cloud",
    },
    NavItem {
        label: "Settings",
        href: "/settings",
        icon: "settings",
    },
];

/// Sidebar component with navigation links
#[component]
pub fn Sidebar() -> impl IntoView {
    view! {
        <aside class="sidebar">
            <div class="sidebar-header">
                <div class="sidebar-logo">
                    <span class="material-symbols-outlined" style="font-size: 1.25rem;">bolt</span>
                </div>
                <span class="sidebar-title">"Hodei Console"</span>
            </div>
            <nav class="sidebar-nav">
                <a href="/" class="nav-item active">
                    <span class="material-symbols-outlined nav-icon">"dashboard"</span>
                    "Dashboard"
                </a>
                <a href="/jobs" class="nav-item">
                    <span class="material-symbols-outlined nav-icon">"work"</span>
                    "Jobs"
                </a>
                <a href="/workers" class="nav-item">
                    <span class="material-symbols-outlined nav-icon">"memory"</span>
                    "Workers"
                </a>
                <a href="/scheduler" class="nav-item">
                    <span class="material-symbols-outlined nav-icon">"schedule"</span>
                    "Scheduler"
                </a>
                <a href="/providers" class="nav-item">
                    <span class="material-symbols-outlined nav-icon">"cloud"</span>
                    "Providers"
                </a>
                <a href="/settings" class="nav-item">
                    <span class="material-symbols-outlined nav-icon">"settings"</span>
                    "Settings"
                </a>
            </nav>
        </aside>
    }
}

/// Main layout wrapper with sidebar
#[component]
pub fn Layout(children: Children) -> impl IntoView {
    view! {
        <div class="app-layout">
            <Sidebar />
            <main class="main-content">
                <header class="header">
                    <div class="header-breadcrumbs">
                        <span class="current">"Hodei Jobs Platform"</span>
                    </div>
                    <div class="header-actions">
                        <div class="header-search">
                            <span class="material-symbols-outlined header-search-icon">"search"</span>
                            <input type="text" placeholder="Search..." />
                        </div>
                        <button class="nav-btn">
                            <span class="material-symbols-outlined">"notifications"</span>
                        </button>
                    </div>
                </header>
                <div class="page-content">
                    {children()}
                </div>
            </main>
        </div>
    }
}
