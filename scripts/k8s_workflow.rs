#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "k8s_workflow"
//! version = "0.1.0"
//! edition = "2024"
//! ```

//! Complete Kubernetes workflow for Hodei Jobs Platform using k3s
//!
//! **Este script es para DESARROLLO con k3s + DevSpace.**
//! k3s es un Kubernetes ligero que incluye containerd integrado.
//!
//! **Configuración previa (una vez)**:
//!   # Instalar k3s
//!   curl -sfL https://get.k3s.io | sh -
//!
//!   # Configurar kubectl sin sudo
//!   sudo chmod +r /etc/rancher/k3s/k3s.yaml
//!   cp /etc/rancher/k3s/k3s.yaml ~/.kube/config
//!   chmod 600 ~/.kube/config
//!   echo 'export KUBECONFIG=~/.kube/config' >> ~/.bashrc
//!
//!   # Crear symlink para usar kubectl directamente
//!   sudo ln -sf /usr/local/bin/k3s /usr/local/bin/kubectl
//!
//!   # Permitir acceso a containerd sin sudo (una vez)
//!   sudo chmod 666 /run/k3s/containerd/containerd.sock
//!
//! **Para producción**, usar directamente Helm con values.yaml:
//!   helm upgrade --install hodei ./deploy/hodei-jobs-platform -n hodei-jobs -f ./deploy/hodei-jobs-platform/values.yaml
//!
//! El script es **idempotente**: puede ejecutarse múltiples veces de forma segura.

use std::process::Command;

fn run_kubectl(args: &[&str]) -> String {
    // Try user's kubectl first (asdf or system), then k3s kubeconfig
    let result = Command::new("kubectl").args(args).output();

    match result {
        Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => {
            // Fallback to k3s kubeconfig
            let fallback = Command::new("env")
                .args(&["KUBECONFIG=/etc/rancher/k3s/k3s.yaml", "kubectl"])
                .args(args)
                .output();
            match fallback {
                Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
                _ => String::new(),
            }
        }
    }
}

fn run_kubectl_status(args: &[&str]) -> bool {
    // Try user's kubectl first (asdf or system), then k3s kubeconfig
    let result = Command::new("kubectl").args(args).status();

    match result {
        Ok(s) if s.success() => true,
        _ => {
            // Fallback to k3s kubeconfig
            Command::new("env")
                .args(&["KUBECONFIG=/etc/rancher/k3s/k3s.yaml", "kubectl"])
                .args(args)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
    }
}

fn run_helm(args: &[&str]) -> bool {
    // Run helm with k3s kubeconfig (asdf helm doesn't have kubeconfig set)
    let status = Command::new("env")
        .args(&["KUBECONFIG=/etc/rancher/k3s/k3s.yaml", "helm"])
        .args(args)
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

fn run_helm_output(args: &[&str]) -> String {
    let result = Command::new("env")
        .args(&["KUBECONFIG=/etc/rancher/k3s/k3s.yaml", "helm"])
        .args(args)
        .output();
    match result {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}

fn main() {
    // Get the project root directory (where the script is located)
    let project_root = std::env::current_dir()
        .expect("Failed to get current directory")
        .to_string_lossy()
        .to_string();

    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - K8S WORKFLOW (k3s)      ║");
    println!("╚═══════════════════════════════════════════╝");
    println!("\n📁 Project root: {}", project_root);
    println!("\n📝 Using k3s (lightweight Kubernetes with containerd)");

    println!("\n🔍 Checking prerequisites...");

    // Check kubectl (from asdf or system)
    let kubectl_status = Command::new("kubectl")
        .args(&["version", "--client"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !kubectl_status {
        println!("   ❌ kubectl not found. Install with:");
        println!("      asdf plugin add kubectl");
        return;
    }
    println!("   ✅ kubectl ready");

    // Check cluster access
    let cluster_check = run_kubectl(&["get", "nodes", "-o", "jsonpath={.items[0].metadata.name}"]);

    if cluster_check.is_empty() {
        println!("   ❌ Cannot access cluster. Check if k3s is running:");
        println!("      systemctl status k3s");
        return;
    }
    println!("   ✅ Cluster accessible: {}", cluster_check);

    // Check helm (use k3s kubectl helm as fallback)
    let helm_available = Command::new("helm")
        .arg("version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !helm_available {
        println!("   ⚠️  helm not found. Install with:");
        println!("      asdf plugin add helm");
        return;
    }
    println!("   ✅ Helm ready (will use k3s kubeconfig)");

    // Check namespaces
    println!("\n📦 Checking namespaces...");
    let ns_output = run_kubectl(&[
        "get",
        "namespace",
        "hodei-jobs",
        "-o",
        "jsonpath={.metadata.name}",
    ]);

    if ns_output.is_empty() {
        println!("   📦 Creating hodei-jobs namespace...");
        let _ = run_kubectl_status(&["create", "namespace", "hodei-jobs"]);
    }
    println!("   ✅ Namespace ready");

    // Build Rust binary
    println!("\n🔨 Building Rust binary...");
    let status = Command::new("cargo")
        .args(&["build", "--release", "-p", "hodei-server-bin"])
        .status();
    if status.map(|s| s.success()).unwrap_or(false) {
        println!("   ✅ Build complete");
    } else {
        println!("   ❌ Build failed");
        return;
    }

    // Check podman or docker for building images
    let (build_cmd, build_name) = if Command::new("podman")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        ("podman".to_string(), "Podman")
    } else if Command::new("docker")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        ("docker".to_string(), "Docker")
    } else {
        println!("   ❌ Neither podman nor docker found. Install one of them:");
        println!("      sudo apt install podman -y");
        return;
    };
    println!("   ✅ {} ready: {}", build_name, build_cmd);

    // Build image (development image with Rust toolchain)
    println!("\n🐳 Building development image...");
    println!("   (Using Dockerfile.dev - includes Rust toolchain with mold/sccache for fast compilation)");

    // Get registry IP/host
    let registry_host = "registry.local:31500";

    // Build and tag image
    let build_status = Command::new(&build_cmd)
        .args(&[
            "build",
            "-f",
            "Dockerfile.dev",
            "-t",
            &format!("{}/hodei-jobs-server:dev", registry_host),
            ".",
        ])
        .status();
    if !build_status.map(|s| s.success()).unwrap_or(false) {
        println!("   ❌ {} build failed", build_name);
        return;
    }
    println!("   ✅ {} image built", build_name);

    // Push to local registry
    println!("\n📤 Pushing image to local registry...");
    let push_status = Command::new(&build_cmd)
        .args(&[
            "push",
            &format!("{}/hodei-jobs-server:dev", registry_host),
            "--tls-verify=false",
        ])
        .status();
    if push_status.map(|s| s.success()).unwrap_or(false) {
        println!("   ✅ Image pushed to {}", registry_host);
    } else {
        println!("   ⚠️  Push failed, continuing anyway...");
    }

    // Deploy using ONLY values-dev.yaml (development configuration)
    println!("\n🚀 Deploying to Kubernetes...");
    println!("   📝 Using: values-dev.yaml (development configuration)");

    // Check if release exists and handle conflicts (idempotency)
    let helm_check = run_helm_output(&["list", "-n", "hodei-jobs", "-o", "json"]);

    if !helm_check.is_empty() && helm_check.contains("hodei") {
        println!("   🔄 Release exists, uninstalling for clean deploy...");
        let _ = run_helm(&["uninstall", "hodei", "-n", "hodei-jobs"]);

        let _ = run_kubectl_status(&[
            "delete",
            "deployment",
            "-n",
            "hodei-jobs",
            "--all",
            "--ignore-not-found",
        ]);

        println!("   ⏳ Waiting for cleanup...");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }

    // Deploy with Helm (using k3s kubeconfig)
    let deploy_status = run_helm(&[
        "upgrade",
        "--install",
        "hodei",
        &format!("{}/deploy/hodei-jobs-platform", project_root),
        "-n",
        "hodei-jobs",
        "-f",
        &format!(
            "{}/deploy/hodei-jobs-platform/values-dev.yaml",
            project_root
        ),
        "--kubeconfig=/etc/rancher/k3s/k3s.yaml",
        "--wait",
        "--timeout",
        "5m",
    ]);

    if deploy_status {
        println!("   ✅ Deployed!");
    } else {
        println!("   ⚠️  Deploy had issues, checking status...");
        let _ = run_kubectl(&[
            "get",
            "pods",
            "-n",
            "hodei-jobs",
            "-l",
            "app.kubernetes.io/name=hodei-jobs-platform",
        ]);
        return;
    }

    // Show status
    println!("\n📊 Deployment status:");
    let _ = run_kubectl(&[
        "get",
        "pods",
        "-n",
        "hodei-jobs",
        "-l",
        "app.kubernetes.io/name=hodei-jobs-platform",
    ]);

    println!("\n✅ k3s development workflow complete!");
    println!("\n💡 Next steps:");
    println!("   - Start DevSpace: just devspace-dev");
    println!("   - Or sync binary: just dev-reload");
    println!("   - View pods: kubectl get pods -n hodei-jobs");
}
