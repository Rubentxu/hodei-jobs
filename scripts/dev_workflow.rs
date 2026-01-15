#!/usr/bin/env rust-script
//! Hodei Jobs - Dev Workflow Script
//!
//! Optimized for Minikube + DevSpace development
//!
//! Usage:
//!   rust-script dev_workflow.rs init   # Initialize dev environment
//!   rust-script dev_workflow.rs build  # Compile + sync to cluster
//!   rust-script dev_workflow.rs reload # Hot reload after changes
//!   rust-script dev_workflow.rs logs   # Stream logs
//!
//! Or install as just commands:
//!   just dev-init
//!   just dev-build
//!   just dev-reload
//!   just dev-logs

use std::env;
use std::fs;
use std::process::{exit, Command};

const NAMESPACE: &str = "hodei-jobs";
const IMAGE: &str = "localhost:5000/hodei-jobs-server:latest";
const BINARY_PATH: &str = "target/release/hodei-server-bin";
const PVC_NAME: &str = "hodei-cargo-cache";

fn log_info(msg: &str) {
    println!("\x1b[32mℹ️  {}\x1b[0m", msg);
}

fn log_warn(msg: &str) {
    println!("\x1b[33m⚠️  {}\x1b[0m", msg);
}

fn log_error(msg: &str) {
    println!("\x1b[31m❌ {}\x1b[0m", msg);
}

fn run_command(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute {}: {}", cmd, e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_kubectl(args: &[&str]) -> Result<String, String> {
    run_command("kubectl", args)
}

fn run_docker(args: &[&str]) -> Result<String, String> {
    run_command("docker", args)
}

fn run_cargo(args: &[&str]) -> Result<String, String> {
    run_command("cargo", args)
}

/// Init: Create cargo cache PVC and build initial image
fn init() -> Result<(), Box<dyn std::error::Error>> {
    log_info("🚀 Initializing development environment...");

    // Create PVC for cargo cache if it doesn't exist
    let pvc_check = run_kubectl(&["get", "pvc", PVC_NAME, "-n", NAMESPACE]);

    if pvc_check.is_err() {
        log_info("Creating cargo cache PVC...");

        // Apply dev manifests from the chart
        let manifests_path = "deploy/hodei-jobs-platform/manifests-dev.yaml";
        if fs::metadata(manifests_path).is_ok() {
            run_kubectl(&["apply", "-f", manifests_path])?;
            log_info("✅ Dev manifests applied (includes cargo cache PVC)");
        } else {
            // Fallback: create PVC directly
            let pvc_manifest = format!(
                r#"
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: {}
  namespace: {}
  labels:
    app.kubernetes.io/name: hodei-jobs-platform
    app.kubernetes.io/component: dev-cache
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 10Gi
  storageClassName: standard
"#,
                PVC_NAME, NAMESPACE
            );
            fs::write("/tmp/cargo-cache-pvc.yaml", &pvc_manifest)?;
            run_kubectl(&["apply", "-f", "/tmp/cargo-cache-pvc.yaml"])?;
            log_info("✅ Cargo cache PVC created");
        }
    } else {
        log_info("✅ Cargo cache PVC already exists");
    }

    // Build and push the base image
    log_info("Building Docker image...");
    run_docker(&["build", "-f", "Dockerfile.server", "-t", IMAGE, "."])?;
    run_docker(&["push", IMAGE])?;
    log_info("✅ Image pushed to registry");

    // Restart deployment to pick up new image
    log_info("Restarting deployment...");
    run_kubectl(&[
        "rollout",
        "restart",
        "deployment/hodei-hodei-jobs-platform",
        "-n",
        NAMESPACE,
    ])?;
    run_kubectl(&[
        "rollout",
        "status",
        "deployment/hodei-hodei-jobs-platform",
        "-n",
        NAMESPACE,
        "--timeout=60s",
    ])?;

    log_info("✅ Initialization complete!");
    log_info("");
    log_info("Next steps:");
    log_info("  1. Start DevSpace: just devspace-dev");
    log_info("  2. Or manually sync: just dev-build");

    Ok(())
}

/// Build: Compile and sync binary to cluster
fn build() -> Result<(), Box<dyn std::error::Error>> {
    log_info("🔨 Compiling release binary...");

    if !fs::metadata(BINARY_PATH).is_ok() {
        log_info("Binary not found, compiling now...");
        run_cargo(&["build", "--release", "-p", "hodei-server-bin"])?;
    } else {
        log_info("Binary found. Rebuilding for latest changes...");
        run_cargo(&["build", "--release", "-p", "hodei-server-bin"])?;
    }

    // Verify binary exists
    let metadata = fs::metadata(BINARY_PATH).map_err(|_| "Binary not found")?;
    log_info(&format!("Binary size: {} bytes", metadata.len()));

    // Find the server pod
    let pod_output = run_kubectl(&[
        "get",
        "pods",
        "-n",
        NAMESPACE,
        "-l",
        "app.kubernetes.io/name=hodei-jobs-platform,app.kubernetes.io/component=server",
        "-o",
        "jsonpath={.items[0].metadata.name}",
    ])?;

    let pod_name = pod_output.trim();
    if pod_name.is_empty() {
        log_error("No server pod found in namespace hodei-jobs");
        log_info("Make sure the deployment is running");
        exit(1);
    }

    log_info(&format!("📤 Syncing binary to pod: {}", pod_name));

    // Copy binary to pod
    run_kubectl(&["cp", BINARY_PATH, &format!("{}:{}", NAMESPACE, pod_name)])?;

    // Restart the server process in the pod
    log_info("🔄 Reloading server...");

    let reload_result = run_kubectl(&[
        "exec",
        "-n",
        NAMESPACE,
        pod_name,
        "--",
        "sh",
        "-c",
        "kill -USR1 $(cat /tmp/server.pid 2>/dev/null)",
    ]);

    if reload_result.is_err() {
        log_warn("Could not reload server - pod may need restart");
    }

    log_info("✅ Build and sync complete!");
    Ok(())
}

/// Reload: Quick reload after small changes
fn reload() -> Result<(), Box<dyn std::error::Error>> {
    log_info("🔄 Quick reload...");

    // Incremental build (faster)
    run_cargo(&["build", "--release", "-p", "hodei-server-bin"])?;
    build()
}

/// Logs: Stream logs from the server pod
fn logs() -> Result<(), Box<dyn std::error::Error>> {
    let pod_output = run_kubectl(&[
        "get",
        "pods",
        "-n",
        NAMESPACE,
        "-l",
        "app.kubernetes.io/name=hodei-jobs-platform,app.kubernetes.io/component=server",
        "-o",
        "jsonpath={.items[0].metadata.name}",
    ])?;

    let pod_name = pod_output.trim();
    if pod_name.is_empty() {
        log_error("No server pod found");
        exit(1);
    }

    log_info("📜 Streaming logs (Ctrl+C to exit)...");

    // Use tail to follow logs
    let status = Command::new("kubectl")
        .args(&["logs", "-f", "-n", NAMESPACE, pod_name, "--tail=100"])
        .status()
        .map_err(|e| format!("Failed to execute kubectl logs: {}", e))?;

    if !status.success() {
        log_error("Failed to stream logs");
    }
    Ok(())
}

/// Shell: Open shell in the server pod
fn shell() -> Result<(), Box<dyn std::error::Error>> {
    let pod_output = run_kubectl(&[
        "get",
        "pods",
        "-n",
        NAMESPACE,
        "-l",
        "app.kubernetes.io/name=hodei-jobs-platform,app.kubernetes.io/component=server",
        "-o",
        "jsonpath={.items[0].metadata.name}",
    ])?;

    let pod_name = pod_output.trim();
    if pod_name.is_empty() {
        log_error("No server pod found");
        exit(1);
    }

    log_info(&format!("Opening shell in pod: {}", pod_name));

    let status = Command::new("kubectl")
        .args(&["exec", "-it", "-n", NAMESPACE, pod_name, "--", "/bin/bash"])
        .status()
        .map_err(|e| format!("Failed to execute kubectl exec: {}", e))?;

    exit(status.code().unwrap_or(1));
}

/// Status: Check development environment status
fn status() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Development Environment Status");
    println!("==================================");

    println!("\n🖥️  Pods:");
    match run_kubectl(&[
        "get",
        "pods",
        "-n",
        NAMESPACE,
        "-l",
        "app.kubernetes.io/name=hodei-jobs-platform",
        "--no-headers",
    ]) {
        Ok(output) => print!("{}", output),
        Err(e) => log_warn(&format!("Could not get pods: {}", e)),
    }

    println!("\n💾 Cargo Cache PVC:");
    match run_kubectl(&["get", "pvc", PVC_NAME, "-n", NAMESPACE]) {
        Ok(output) => print!("{}", output),
        Err(e) => log_warn(&format!("Could not get PVC: {}", e)),
    }

    println!("\n📦 Binary:");
    match fs::metadata(BINARY_PATH) {
        Ok(metadata) => println!("  {} bytes", metadata.len()),
        Err(_) => println!("  Not found"),
    }

    println!("\n🐳 Images:");
    match run_docker(&[
        "images",
        "localhost:5000/hodei-jobs-server",
        "--format",
        "table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}",
    ]) {
        Ok(output) => print!("{}", output),
        Err(e) => log_warn(&format!("Could not get images: {}", e)),
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("build");

    let result = match command {
        "init" => init(),
        "build" => build(),
        "reload" => reload(),
        "logs" => logs(),
        "shell" => shell(),
        "status" => status(),
        _ => {
            println!("Usage: {} {{init|build|reload|logs|shell|status}}", args[0]);
            println!("");
            println!("Commands:");
            println!("  init    - Initialize dev environment (PVC + image)");
            println!("  build   - Compile and sync binary to cluster");
            println!("  reload  - Quick reload after small changes");
            println!("  logs    - Stream logs from server pod");
            println!("  shell   - Open shell in server pod");
            println!("  status  - Check dev environment status");
            exit(1);
        }
    };

    if let Err(e) = result {
        log_error(&format!("Error: {}", e));
        exit(1);
    }
}
