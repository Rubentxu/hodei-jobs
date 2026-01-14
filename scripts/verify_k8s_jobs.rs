#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "verify_k8s_jobs"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Verify Kubernetes jobs

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - VERIFY K8S JOBS          ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n📊 Kubernetes Jobs:");
    let _ = Command::new("kubectl")
        .args(&["get", "jobs", "-n", "hodei-jobs", "-o", "wide"])
        .status();

    println!("\n📊 CronJobs:");
    let _ = Command::new("kubectl")
        .args(&["get", "cronjobs", "-n", "hodei-jobs", "-o", "wide"])
        .status();

    println!("\n💡 To create a job:");
    println!("   kubectl create job test-job -n hodei-jobs --image=localhost:5000/hodei-jobs-worker:latest");
}
