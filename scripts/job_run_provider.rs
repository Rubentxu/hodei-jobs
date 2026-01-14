#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "job_run_provider"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Run a job with specific provider

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - RUN JOB (PROVIDER)       ║");
    println!("╚═══════════════════════════════════════════╝");
    println!("\n💡 Run a job with specific provider:");
    println!();
    println!("   cargo run --bin hodei-jobs-cli -- job run \\");
    println!("     --name \"My Job\" \\");
    println!("     --command \"python script.py\" \\");
    println!("     --provider <provider-name>");
    println!();
    println!("   Available providers: docker, kubernetes");
}
