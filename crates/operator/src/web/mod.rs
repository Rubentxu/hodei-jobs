//! Web Dashboard Module
//!
//! Leptos-based Dashboard for Hodei Operator
//! Can be served as static WASM or with live data.

use serde::{Deserialize, Serialize};

pub mod server;

// Re-export types for external use
pub use server::DashboardStats;

// ============ API Types ============

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Job {
    pub name: String,
    pub namespace: String,
    pub command: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkerPool {
    pub name: String,
    pub namespace: String,
    pub provider: String,
    pub min_replicas: i32,
    pub max_replicas: i32,
    pub current_replicas: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderConfig {
    pub name: String,
    pub namespace: String,
    pub provider_type: String,
    pub enabled: bool,
}

// ============ CSS Styles ============

pub const STYLES: &str = r#"
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f5f5f5; color: #333; }
    .app { min-height: 100vh; }
    .navbar { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); padding: 1rem 2rem; display: flex; align-items: center; justify-content: space-between; color: white; }
    .nav-brand { display: flex; align-items: center; gap: 0.5rem; }
    .logo { font-size: 1.5rem; }
    .title { font-size: 1.25rem; font-weight: 600; }
    .nav-links { display: flex; gap: 1rem; }
    .nav-link { color: rgba(255,255,255,0.8); text-decoration: none; padding: 0.5rem 1rem; border-radius: 6px; transition: all 0.2s; }
    .nav-link:hover, .nav-link.active { background: rgba(255,255,255,0.2); color: white; }
    .nav-status { display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem; }
    .status-dot { width: 8px; height: 8px; background: #10b981; border-radius: 50%; }
    main { padding: 2rem; max-width: 1400px; margin: 0 auto; }
    h1 { font-size: 1.75rem; margin-bottom: 1.5rem; }
    h2 { font-size: 1.25rem; margin-bottom: 1rem; }
    .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 1rem; margin-bottom: 2rem; }
    .stat-card { background: white; padding: 1.5rem; border-radius: 10px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
    .stat-card h3 { color: #666; font-size: 0.875rem; margin-bottom: 0.5rem; }
    .stat-card .value { font-size: 2rem; font-weight: 700; color: var(--card-color, #333); }
    .sections-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 1.5rem; }
    .section { background: white; border-radius: 10px; padding: 1.5rem; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
    .data-table { width: 100%; border-collapse: collapse; }
    .data-table th, .data-table td { padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid #e5e7eb; }
    .data-table th { background: #f9fafb; font-weight: 600; color: #666; font-size: 0.75rem; text-transform: uppercase; }
    .data-table tr:hover { background: #f9fafb; }
    .data-table code { background: #f3f4f6; padding: 0.25rem 0.5rem; border-radius: 4px; }
    .status-badge { padding: 0.25rem 0.75rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 500; }
    .status-running { background: #d1fae5; color: #065f46; }
    .status-pending { background: #fef3c7; color: #92400e; }
    .status-completed { background: #dbeafe; color: #1e40af; }
    .status-failed { background: #fee2e2; color: #991b1b; }
    .btn { padding: 0.5rem 1rem; border: none; border-radius: 6px; cursor: pointer; font-size: 0.875rem; transition: all 0.2s; }
    .btn-primary { background: #667eea; color: white; }
    .btn-primary:hover { background: #5a67d8; }
    .page-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1.5rem; }
    .pools-grid, .providers-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 1rem; }
    .pool-card, .provider-card { background: white; border-radius: 10px; padding: 1.25rem; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
    .pool-header, .provider-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; }
    .pool-header h3, .provider-header h3 { font-size: 1rem; }
    .provider-badge { background: #e5e7eb; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; }
    .pool-stats { display: flex; gap: 1.5rem; }
    .pool-stat { display: flex; flex-direction: column; }
    .pool-stat .label { font-size: 0.75rem; color: #666; }
    .pool-stat .value { font-weight: 600; }
    .enabled-badge { padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; }
    .enabled-badge.enabled { background: #d1fae5; color: #065f46; }
    .enabled-badge.disabled { background: #fee2e2; color: #991b1b; }
    .stats-list, .status-list { list-style: none; }
    .stats-list li, .status-list li { padding: 0.5rem 0; border-bottom: 1px solid #e5e7eb; display: flex; justify-content: space-between; }
    .status-list li.ok { color: #10b981; }
"#;
