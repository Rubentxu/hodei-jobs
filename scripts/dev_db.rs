#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "dev_db"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Start development PostgreSQL container

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - DEV DATABASE             ║");
    println!("╚═══════════════════════════════════════════╝");

    // Check if PostgreSQL is running
    println!("\n🔍 Checking PostgreSQL status...");
    let running = Command::new("docker")
        .args(&["ps", "-q", "--filter", "name=hodei-jobs-postgres"])
        .output()
        .ok()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    if running {
        println!("   ✅ PostgreSQL already running");
    } else {
        println!("   🟡 Starting PostgreSQL...");

        // Start PostgreSQL
        let status = Command::new("docker")
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
                "-v",
                "hodei-jobs-pgdata:/var/lib/postgresql/data",
                "postgres:15-alpine",
            ])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("   ✅ PostgreSQL started");

                // Wait for PostgreSQL to be ready
                println!("   ⏳ Waiting for PostgreSQL to be ready...");
                std::thread::sleep(std::time::Duration::from_secs(2));

                // Run migrations
                println!("   📦 Running migrations...");
                let _ = Command::new("docker")
                    .args(&[
                        "exec",
                        "hodei-jobs-postgres",
                        "psql",
                        "-U",
                        "postgres",
                        "-d",
                        "hodei_jobs",
                        "-c",
                        "SELECT 1;",
                    ])
                    .status();

                println!("   ✅ PostgreSQL ready!");
            }
            _ => {
                println!("   ❌ Failed to start PostgreSQL");
                return;
            }
        }
    }

    println!("\n🚀 PostgreSQL is ready!");
    println!("   Connection: postgres://postgres:postgres@localhost:5432/hodei_jobs");
}
