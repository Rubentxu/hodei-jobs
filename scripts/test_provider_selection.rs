#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "test_provider_selection"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Test provider selection algorithm

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - PROVIDER SELECTION TEST  ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n📊 Available Providers:");
    let providers = Command::new("docker")
        .args(&[
            "exec",
            "hodei-jobs-postgres",
            "psql",
            "-U",
            "postgres",
            "-c",
            "SELECT id, name, provider_type, max_workers, status FROM provider_configs;",
        ])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&providers.stdout));

    println!("\n📊 Workers by Provider:");
    let workers = Command::new("docker")
        .args(&["exec", "hodei-jobs-postgres", "psql", "-U", "postgres", "-c",
            "SELECT p.name, COUNT(w.id) as workers, SUM(p.max_workers) as capacity FROM provider_configs p LEFT JOIN workers w ON p.id = w.provider_id GROUP BY p.name;"])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&workers.stdout));

    println!("\n💡 Provider selection considers:");
    println!("   - Provider status (ACTIVE/INACTIVE)");
    println!("   - Available capacity (max_workers - current workers)");
    println!("   - Provider health");
}
