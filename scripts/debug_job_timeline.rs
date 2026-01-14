#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "debug_job_timeline"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Show job execution timeline

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - JOB TIMELINE             ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n📊 RECENT JOB TIMELINE:");
    println!("{}", "-".repeat(70));
    let output = Command::new("docker")
        .args(&["exec", "hodei-jobs-postgres", "psql", "-U", "postgres", "-c", r#"
            SELECT
                id,
                state,
                created_at,
                CASE
                    WHEN started_at IS NOT NULL THEN EXTRACT(EPOCH FROM (started_at - created_at))::int
                    ELSE NULL
                END as queue_time_sec,
                CASE
                    WHEN finished_at IS NOT NULL THEN EXTRACT(EPOCH FROM (finished_at - started_at))::int
                    ELSE NULL
                END as execution_time_sec
            FROM jobs
            ORDER BY created_at DESC
            LIMIT 10;"#])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));

    println!("\n💡 Legend:");
    println!("   queue_time_sec: Seconds from creation to start");
    println!("   execution_time_sec: Seconds from start to finish");
}
