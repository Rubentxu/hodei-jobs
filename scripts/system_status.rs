#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "system_status"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! System Status Dashboard for Hodei Jobs Platform

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

fn print_section(title: &str) {
    println!("\n{}", title);
    println!("{}", "-".repeat(60));
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           HODEI JOBS SYSTEM STATUS                       ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    // Jobs by state
    print_section("📊 JOBS BY STATE:");
    let jobs = run_psql(
        "
        SELECT state || ' | ' || COUNT(*)::text as info
        FROM jobs GROUP BY state ORDER BY
        CASE state
            WHEN 'PENDING' THEN 1 WHEN 'RUNNING' THEN 2
            WHEN 'SUCCEEDED' THEN 3 WHEN 'FAILED' THEN 4
            ELSE 5 END;
    ",
    );
    for line in jobs.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }

    // Workers by state
    print_section("👥 WORKERS BY STATE:");
    let workers = run_psql(
        "
        SELECT state || ' | ' || COUNT(*)::text as info
        FROM workers GROUP BY state ORDER BY state;
    ",
    );
    for line in workers.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }

    // Job queue
    print_section("📋 JOB QUEUE:");
    let queue_count = run_psql("SELECT COUNT(*) FROM job_queue;")
        .parse::<i32>()
        .unwrap_or(0);
    println!("   Total jobs in queue: {}", queue_count);

    // Providers
    print_section("⚙️  PROVIDERS:");
    let providers =
        run_psql("SELECT name || ' | ' || provider_type || ' | ' FROM provider_configs;");
    for line in providers.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }

    // Bootstrap tokens
    print_section("🔑 BOOTSTRAP TOKENS:");
    let tokens = run_psql("SELECT 'Total: ' || COUNT(*)::text || ' | Available: ' || COUNT(CASE WHEN consumed_at IS NULL AND expires_at > now() THEN 1 END)::text FROM worker_bootstrap_tokens;");
    for line in tokens.lines() {
        if !line.is_empty() {
            println!("   {}", line);
        }
    }

    // Processes
    print_section("🔄 PROCESSES:");
    let server_pid = Command::new("pgrep")
        .arg("-f")
        .arg("hodei-server-bin")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(pid) = server_pid {
        println!("   ✅ Server running (PID: {})", pid);
    } else {
        println!("   ❌ Server NOT running");
    }

    let worker_count = Command::new("pgrep")
        .args(&["-c", "-f", "hodei-worker-bin"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0);
    if worker_count > 0 {
        println!("   ✅ Workers running ({} processes)", worker_count);
    } else {
        println!("   ❌ Workers NOT running");
    }

    let postgres = Command::new("docker")
        .args(&["ps", "-q", "--filter", "name=hodei-jobs-postgres"])
        .output()
        .ok()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);
    if postgres {
        println!("   ✅ PostgreSQL running");
    } else {
        println!("   ❌ PostgreSQL NOT running");
    }

    // Metrics
    print_section("📈 METRICS (last hour):");
    let jobs_hour =
        run_psql("SELECT COUNT(*) FROM jobs WHERE created_at > now() - interval '1 hour';");
    let running = run_psql("SELECT COUNT(*) FROM jobs WHERE state = 'RUNNING';");
    let failed_hour = run_psql("SELECT COUNT(*) FROM jobs WHERE state = 'FAILED' AND created_at > now() - interval '1 hour';");
    println!("   Jobs last hour: {}", jobs_hour);
    println!("   Running now: {}", running);
    println!("   Failed last hour: {}", failed_hour);

    println!("\n{}", "=".repeat(60));
    println!("💡 Useful commands:");
    println!("   just debug-system    - Show this dashboard");
    println!("   just logs-server     - Server logs");
    println!("   just restart-all     - Restart system");
    println!("{}", "=".repeat(60));
}
