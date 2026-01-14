#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "build_local"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Build Hodei Jobs Platform locally

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - LOCAL BUILD              ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n🔨 Building workspace...");
    let status = Command::new("cargo")
        .args(&["build", "--workspace"])
        .status();

    if status.map(|s| s.success()).unwrap_or(false) {
        println!("✅ Build complete!");
        println!("\nBinaries:");
        println!("   target/debug/hodei-server-bin");
        println!("   target/debug/hodei-worker-bin");
        println!("   target/debug/hodei-jobs-cli");
    } else {
        println!("❌ Build failed");
    }
}
