#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "dev_start"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Start development environment

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - DEV START                ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n🚀 Starting development environment...");

    // Start PostgreSQL
    println!("1. Starting PostgreSQL...");
    let pg = Command::new("docker")
        .args(&["ps", "-q", "--filter", "name=hodei-jobs-postgres"])
        .output()
        .ok()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);
    if !pg {
        let _ = Command::new("docker")
            .args(&[
                "run",
                "-d",
                "--name",
                "hodei-jobs-postgres",
                "-e",
                "POSTGRES_PASSWORD=postgres",
                "-e",
                "POSTGRES_USER=postgres",
                "-e",
                "POSTGRES_DB=hodei_jobs",
                "-p",
                "5432:5432",
                "postgres:15-alpine",
            ])
            .status();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    println!("   ✅ PostgreSQL ready");

    // Start server
    println!("2. Starting server...");
    let _ = Command::new("bash")
        .arg("-c")
        .arg(r#"cd /home/rubentxu/Proyectos/rust/hodei-jobs && DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs" cargo run -p hodei-server-bin > /tmp/server.log 2>&1 &"#)
        .status();
    println!("   ✅ Server started (logs: tail -f /tmp/server.log)");

    println!("\n✅ Development environment ready!");
    println!("   Server: postgres://postgres:postgres@localhost:5432/hodei_jobs");
}
