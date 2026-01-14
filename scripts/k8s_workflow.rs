#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "k8s_workflow"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Complete Kubernetes workflow for Hodei Jobs Platform

use std::process::Command;

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - K8S WORKFLOW             ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n🔍 Checking prerequisites...");
    let minikube_status = Command::new("minikube")
        .arg("status")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !minikube_status {
        println!("   ❌ Minikube not running. Start with: just setup-minikube");
        return;
    }
    println!("   ✅ Minikube ready");

    // Check namespaces
    println!("\n📦 Checking namespaces...");
    let ns_output = Command::new("kubectl")
        .args(&[
            "get",
            "namespace",
            "hodei-jobs",
            "-o",
            "jsonpath={.metadata.name}",
        ])
        .output()
        .ok()
        .and_then(|o| Some(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        .unwrap_or_default();

    if ns_output.is_empty() {
        println!("   📦 Creating hodei-jobs namespace...");
        let _ = Command::new("kubectl")
            .args(&["create", "namespace", "hodei-jobs"])
            .status();
    }
    println!("   ✅ Namespace ready");

    // Build images
    println!("\n🔨 Building images...");
    let status = Command::new("cargo")
        .args(&["build", "--release", "-p", "hodei-server-bin"])
        .status();
    if status.map(|s| s.success()).unwrap_or(false) {
        println!("   ✅ Build complete");
    } else {
        println!("   ❌ Build failed");
        return;
    }

    // Build and load Docker image
    println!("\n🐳 Building Docker image...");
    let build_status = Command::new("docker")
        .args(&[
            "build",
            "-f",
            "Dockerfile.server",
            "-t",
            "localhost:5000/hodei-jobs-server:latest",
            ".",
        ])
        .status();
    if build_status.map(|s| s.success()).unwrap_or(false) {
        println!("   ✅ Docker image built");
    } else {
        println!("   ❌ Docker build failed");
        return;
    }

    // Load to minikube
    println!("\n📦 Loading to minikube...");
    let _ = Command::new("minikube")
        .args(&["image", "load", "localhost:5000/hodei-jobs-server:latest"])
        .status();
    println!("   ✅ Image loaded");

    // Deploy
    println!("\n🚀 Deploying to Kubernetes...");
    let deploy_status = Command::new("helm")
        .args(&[
            "upgrade",
            "--install",
            "hodei",
            "./deploy/hodei-jobs-platform",
            "-n",
            "hodei-jobs",
            "-f",
            "./deploy/hodei-jobs-platform/values-dev.yaml",
            "--wait",
            "--timeout",
            "5m",
        ])
        .status();
    if deploy_status.map(|s| s.success()).unwrap_or(false) {
        println!("   ✅ Deployed!");
    } else {
        println!("   ❌ Deploy failed");
        return;
    }

    // Show status
    println!("\n📊 Deployment status:");
    let _ = Command::new("kubectl")
        .args(&[
            "get",
            "pods",
            "-n",
            "hodei-jobs",
            "-l",
            "app.kubernetes.io/name=hodei-jobs-platform",
        ])
        .status();

    println!("\n✅ K8s workflow complete!");
}
