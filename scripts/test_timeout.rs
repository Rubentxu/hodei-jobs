#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "test_timeout"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Test job timeout functionality

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - TIMEOUT TEST             ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n📊 Jobs with timeouts:");
    let output = Command::new("docker")
        .args(&["exec", "hodei-jobs-postgres", "psql", "-U", "postgres", "-c",
            "SELECT id, command, timeout_seconds, state FROM jobs WHERE timeout_seconds IS NOT NULL LIMIT 10;"])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));

    println!("\n💡 To create a job with timeout:");
    println!("   cargo run --bin hodei-jobs-cli -- job run \\");
    println!("     --name \"Long Job\" \\");
    println!("     --command \"sleep 300\" \\");
    println!("     --timeout 60");
}
