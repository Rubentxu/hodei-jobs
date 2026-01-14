#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "debug_job"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Debug job status and details

use std::process::Command;

fn run_psql(query: &str) -> String {
    let output = Command::new("docker")
        .args(&[
            "exec",
            "hodei-jobs-postgres",
            "psql",
            "-U",
            "postgres",
            "-t",
            "-c",
            query,
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - DEBUG JOBS               ║");
    println!("╚═══════════════════════════════════════════╝");

    // Jobs by state
    println!("\n📊 JOBS BY STATE:");
    println!("{}", "-".repeat(50));
    let states = run_psql("SELECT state, COUNT(*) FROM jobs GROUP BY state ORDER BY state;");
    for line in states.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }

    // Pending jobs
    println!("\n⏳ PENDING JOBS:");
    println!("{}", "-".repeat(50));
    let pending = run_psql("SELECT id, created_at, command FROM jobs WHERE state = 'PENDING' ORDER BY created_at DESC LIMIT 10;");
    for line in pending.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }

    // Running jobs
    println!("\n🔄 RUNNING JOBS:");
    println!("{}", "-".repeat(50));
    let running = run_psql("SELECT j.id, j.command, w.id as worker_id FROM jobs j JOIN workers w ON j.id = w.current_job_id WHERE j.state = 'RUNNING';");
    for line in running.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }

    // Failed jobs
    println!("\n❌ FAILED JOBS (last 5):");
    println!("{}", "-".repeat(50));
    let failed = run_psql("SELECT id, command, error_message FROM jobs WHERE state = 'FAILED' ORDER BY updated_at DESC LIMIT 5;");
    for line in failed.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }
}
