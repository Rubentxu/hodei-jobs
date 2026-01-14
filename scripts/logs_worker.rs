#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "logs_worker"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Show worker logs

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - WORKER LOGS              ║");
    println!("╚═══════════════════════════════════════════╝");
    println!("\n💡 Run: docker logs -f hodei-worker");
    println!("   Or: kubectl logs -n hodei-jobs -l app.kubernetes.io/name=hodei-worker");
}
