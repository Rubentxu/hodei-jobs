#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "build_and_push"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Build and push Docker images to registry

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - BUILD AND PUSH           ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n🐳 Building server image...");
    let status = Command::new("docker")
        .args(&[
            "build",
            "-f",
            "Dockerfile.server",
            "-t",
            "localhost:5000/hodei-jobs-server:latest",
            ".",
        ])
        .status();
    if status.map(|s| s.success()).unwrap_or(false) {
        println!("   ✅ Server image built");
    } else {
        println!("   ❌ Build failed");
        return;
    }

    println!("\n🐳 Building worker image...");
    let status = Command::new("docker")
        .args(&[
            "build",
            "-f",
            "Dockerfile.worker",
            "-t",
            "localhost:5000/hodei-jobs-worker:latest",
            ".",
        ])
        .status();
    if status.map(|s| s.success()).unwrap_or(false) {
        println!("   ✅ Worker image built");
    } else {
        println!("   ❌ Build failed");
        return;
    }

    println!("\n📤 Pushing to registry...");
    let _ = Command::new("docker")
        .args(&["push", "localhost:5000/hodei-jobs-server:latest"])
        .status();
    let _ = Command::new("docker")
        .args(&["push", "localhost:5000/hodei-jobs-worker:latest"])
        .status();

    println!("\n✅ Images pushed to localhost:5000");
}
