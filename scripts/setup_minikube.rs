#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "setup_minikube"
//! version = "0.1.0"
//! edition = "2024"
//!
//! [dependencies]
//! anyhow = "1.0"
//! ```

//! Setup Minikube for Hodei Jobs Platform

use anyhow::{bail, Result};
use std::process::Command;

const ADDONS: &[&str] = &[
    "storage-provisioner",
    "default-storageclass",
    "registry",
    "registry-aliases",
    "metrics-server",
    "kong",
    "ingress-dns",
];

fn check_minikube() -> Result<()> {
    let status = Command::new("which").arg("minikube").status()?;
    if !status.success() {
        bail!("minikube not installed. Install from: https://minikube.sigs.k8s.io/docs/start/");
    }
    Ok(())
}

fn is_running() -> bool {
    Command::new("minikube")
        .arg("status")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn start_minikube() -> Result<()> {
    println!("🟡 Starting minikube...");
    let status = Command::new("minikube")
        .args(&[
            "start",
            "--cpus=4",
            "--memory=8192",
            "--disk-size=40g",
            "--driver=docker",
            "--kubernetes-version=stable",
        ])
        .status()?;
    if !status.success() {
        bail!("Failed to start minikube");
    }
    Ok(())
}

fn enable_addons() {
    println!("\n📦 Enabling addons...");
    for addon in ADDONS {
        let output = Command::new("minikube")
            .args(&["addons", "list", "-o", "json"])
            .output()
            .unwrap();
        let json = String::from_utf8_lossy(&output.stdout);
        let is_enabled = json.contains(&format!(r#""{}":"#, addon)) && json.contains("enabled");

        if is_enabled {
            println!("  ✅ {} (enabled)", addon);
        } else {
            let _ = Command::new("minikube")
                .args(&["addons", "enable", addon])
                .status();
            println!("  📦 {} (enabling)", addon);
        }
    }
}

fn create_namespace(ns: &str) {
    // Create namespace if it doesn't exist
    let _ = Command::new("kubectl")
        .args(&["get", "namespace", ns])
        .stdout(std::process::Stdio::null())
        .status();

    // Try to create if it doesn't exist
    let check = Command::new("kubectl")
        .args(&["get", "namespace", ns, "-o", "jsonpath={.metadata.name}"])
        .output();

    if let Ok(out) = check {
        if String::from_utf8_lossy(&out.stdout).trim().is_empty() {
            let _ = Command::new("kubectl")
                .args(&["create", "namespace", ns])
                .status();
        }
    }
    println!("  ✅ {} namespace ready", ns);
}

fn show_status() {
    let ip = Command::new("minikube")
        .arg("ip")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    println!("\n╔═══════════════════════════════════════════╗");
    println!("║     MINIKUBE SETUP COMPLETE               ║");
    println!("╚═══════════════════════════════════════════╝");
    println!("\n📊 Minikube IP: {}", ip);
    println!("\n🚀 Next Steps:");
    println!("  just build-minikube   # Build and load images");
    println!("  just deploy           # Deploy to k8s");
    println!("  kubectl get pods -n hodei-jobs -w   # Watch pods");
}

fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - MINIKUBE SETUP           ║");
    println!("╚═══════════════════════════════════════════╝");

    println!("\n🔍 Checking prerequisites...");
    if let Err(e) = check_minikube() {
        eprintln!("❌ {}", e);
        std::process::exit(1);
    }
    println!("  ✅ minikube found");

    if is_running() {
        println!("  ✅ minikube is already running");
    } else {
        if let Err(e) = start_minikube() {
            eprintln!("❌ {}", e);
            std::process::exit(1);
        }
    }

    enable_addons();

    println!("\n📦 Creating namespaces...");
    create_namespace("hodei-jobs");
    create_namespace("hodei-jobs-workers");

    show_status();
}
