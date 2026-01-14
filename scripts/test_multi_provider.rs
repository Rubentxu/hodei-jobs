#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "test_multi_provider"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Test multi-provider functionality

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - MULTI-PROVIDER TEST      ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n🔍 Provider Status:");
    let providers = Command::new("docker")
        .args(&[
            "exec",
            "hodei-jobs-postgres",
            "psql",
            "-U",
            "postgres",
            "-t",
            "-c",
            "SELECT name, provider_type, status FROM provider_configs ORDER BY name;",
        ])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&providers.stdout));

    println!("\n📊 Workers by Provider:");
    let workers = Command::new("docker")
        .args(&["exec", "hodei-jobs-postgres", "psql", "-U", "postgres", "-t", "-c",
            "SELECT p.name, COUNT(w.id) as worker_count FROM provider_configs p LEFT JOIN workers w ON p.id = w.provider_id GROUP BY p.name;"])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&workers.stdout));

    println!("\n💡 To enable Kubernetes provider:");
    println!("   just k8s-enable");
    println!("\n💡 To test specific provider:");
    println!("   just test-provider-selection");
}
