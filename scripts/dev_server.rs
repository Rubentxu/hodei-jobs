#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "dev_server"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Start Hodei server in development mode

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - DEV SERVER               ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n🚀 Starting development server...");
    println!("   Database: postgres://postgres:postgres@localhost:5432/hodei_jobs");
    println!("   Logs: /tmp/server.log");
    println!();

    // Set environment and start server
    let status = Command::new("bash")
        .arg("-c")
        .arg(
            r#"
            export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
            export RUST_LOG="debug"
            export HODEI_K8S_ENABLED="0"
            export HODEI_DOCKER_ENABLED="1"
            cd /home/rubentxu/Proyectos/rust/hodei-jobs
            cargo run -p hodei-server-bin 2>&1 | tee /tmp/server.log
        "#,
        )
        .status();

    if status.map(|s| s.success()).unwrap_or(false) {
        println!("\n✅ Server stopped");
    } else {
        println!("\n❌ Server error");
    }
}
