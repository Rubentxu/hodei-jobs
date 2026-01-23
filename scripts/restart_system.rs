#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "restart_system"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Restart Hodei Jobs Platform system

use std::process::Command;

fn kill_process(pattern: &str) {
    let _ = Command::new("pkill").args(&["-9", "-f", pattern]).status();
}

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - RESTART SYSTEM           ║");
    println!("╚═══════════════════════════════════════════╝");

    // Stop workers
    println!("\n1. Stopping workers...");
    kill_process("hodei-worker-bin");
    let _ = Command::new("docker")
        .args(&["rm", "-f"])
        .args(&["hodei-worker"])
        .status();
    println!("   ✅ Workers stopped");

    // Stop server
    println!("2. Stopping server...");
    kill_process("hodei-server-bin");
    println!("   ✅ Server stopped");

    // Start PostgreSQL
    println!("3. Starting PostgreSQL...");
    let pg_running = Command::new("docker")
        .args(&["ps", "-q", "--filter", "name=hodei-jobs-postgres"])
        .output()
        .ok()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    if !pg_running {
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
    println!("4. Starting server...");
    let server_cmd = " HODEI_DATABASE_URL=postgres://postgres:postgres@localhost:5432/hodei_jobs \
        cargo run --bin hodei-server-bin > /tmp/server.log 2>&1 &";
    let _ = Command::new("bash")
        .arg("-c")
        .arg(&format!(
            "cd /home/rubentxu/Proyectos/rust/hodei-jobs{}",
            server_cmd
        ))
        .status();
    println!("   ✅ Server started");

    println!("\n✅ System restarted!");
    println!("   Logs: tail -f /tmp/server.log");
}
