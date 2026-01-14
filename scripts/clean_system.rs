#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "clean_system"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Clean Hodei Jobs Platform system

use std::process::Command;

fn run_cmd(name: &str, cmd: &str, args: &[&str]) {
    print!("{}...", name);
    let _ = Command::new(cmd).args(args).status();
    println!(" ✅");
}

fn kill_process(pattern: &str) {
    let _ = Command::new("pkill").args(&["-9", "-f", pattern]).status();
}

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - CLEAN SYSTEM             ║");
    println!("╚═══════════════════════════════════════════╝");
    println!();

    // 1. Kill workers
    println!("1. Killing hodei-worker-bin processes...");
    kill_process("hodei-worker-bin");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 2. Kill server
    println!("2. Killing hodei-server-bin processes...");
    kill_process("hodei-server-bin");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 3. Clean Docker containers
    println!("3. Cleaning old Docker containers...");
    let output = Command::new("docker")
        .args(&["ps", "-aq", "--filter", "name=hodei-worker"])
        .output()
        .unwrap();
    let containers_str = String::from_utf8_lossy(&output.stdout);
    let containers = containers_str.trim();
    if !containers.is_empty() {
        let count = containers.split_whitespace().count();
        println!("   Removing {} worker containers...", count);
        let container_list: Vec<&str> = containers.split_whitespace().collect();
        let _ = Command::new("docker")
            .args(&["rm", "-f"])
            .args(&container_list)
            .status();
    }

    // 4. Clean database
    println!("4. Cleaning database...");
    let status = Command::new("docker")
        .args(&["ps", "-q", "--filter", "name=hodei-jobs-postgres"])
        .status();
    if status.map(|s| s.success()).unwrap_or(false) {
        let cmds = &[
            "DROP TABLE IF EXISTS job_queue CASCADE;",
            "DROP TABLE IF EXISTS jobs CASCADE;",
            "DROP TABLE IF EXISTS workers CASCADE;",
            "DROP TABLE IF EXISTS worker_bootstrap_tokens CASCADE;",
            "DROP TABLE IF EXISTS provider_configs CASCADE;",
        ];
        for cmd in cmds {
            let _ = Command::new("docker")
                .args(&[
                    "exec",
                    "hodei-jobs-postgres",
                    "psql",
                    "-U",
                    "postgres",
                    "-c",
                    cmd,
                ])
                .status();
        }
        println!("   ✅ Database cleaned");
    } else {
        println!("   ⚠️  PostgreSQL not running, skipping DB cleanup");
    }

    // 5. Clean temp logs
    println!("5. Cleaning temp logs...");
    let _ = Command::new("rm")
        .args(&["-f", "/tmp/server*.log", "/tmp/worker*.log"])
        .status();
    println!("   ✅ Temp logs cleaned");

    println!();
    println!("✅ SYSTEM CLEANED COMPLETELY");
    println!();
    println!("💡 Next step: just restart-system");
}
