#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "job_run_docker"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Run a job using Docker provider

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - RUN JOB (DOCKER)         ║");
    println!("╚═══════════════════════════════════════════╝");
    println!("\n💡 Use the CLI to run jobs:");
    println!();
    println!("   cargo run --bin hodei-jobs-cli -- job run \\");
    println!("     --name \"Test Job\" \\");
    println!("     --command \"echo 'Hello'\" \\");
    println!("     --provider docker");
    println!();
    println!("   Or via gRPC API using grpcurl or similar tool.");
}
