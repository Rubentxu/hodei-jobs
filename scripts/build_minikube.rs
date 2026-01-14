#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! name = "build_minikube"
//! version = "0.1.0"
//! edition = "2024"
//!
//! [dependencies]
//! anyhow = "1.0"
//! ```

//! Build script for Hodei Jobs Platform - Minikube

use anyhow::Result;
use std::env;
use std::path::PathBuf;
use std::process::Command;

const REGISTRY: &str = "localhost:5000";
const TAG: &str = "latest";

#[derive(Debug, Default, Clone)]
struct Config {
    build_server: bool,
    build_worker: bool,
    no_cache: bool,
}

fn print_help() {
    println!(
        r#"Build script for Hodei Jobs Platform - Minikube

Usage: build_minikube [OPTIONS]

Options:
  --server-only   Build only the server image
  --worker-only   Build only the worker image
  --no-cache      Rebuild without using cache
  --help, -h      Show this help message

Examples:
  build_minikube              # Build both server and worker
  build_minikube --server-only
  build_minikube --worker-only
  build_minikube --no-cache
"#
    );
}

fn parse_args() -> Result<Config> {
    let mut config = Config::default();
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--server-only" => {
                config.build_server = true;
                config.build_worker = false;
            }
            "--worker-only" => {
                config.build_server = false;
                config.build_worker = true;
            }
            "--no-cache" => {
                config.no_cache = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    if !config.build_server && !config.build_worker {
        config.build_server = true;
        config.build_worker = true;
    }

    Ok(config)
}

fn get_project_root() -> PathBuf {
    let current_dir = env::current_dir().unwrap();
    if current_dir.to_str().unwrap_or("").ends_with("scripts") {
        current_dir.parent().unwrap().to_path_buf()
    } else {
        current_dir
    }
}

fn build_image(
    dockerfile: &str,
    image_name: &str,
    context: &PathBuf,
    no_cache: bool,
) -> Result<()> {
    println!("🔨 Building {}:{}...", image_name, TAG);

    let mut args = vec!["build".to_string()];

    if no_cache {
        args.push("--no-cache".to_string());
    }

    let tag_image = format!("{}:{}", image_name, TAG);
    args.extend(vec![
        "-f".to_string(),
        dockerfile.to_string(),
        "-t".to_string(),
        tag_image.clone(),
    ]);

    if !no_cache {
        args.extend(vec!["--cache-from".to_string(), tag_image]);
    }

    args.push(context.to_str().unwrap().to_string());

    let status = Command::new("docker")
        .args(&args)
        .current_dir(context)
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to build {}", image_name);
    }

    println!("✅ Built {}:{}", image_name, TAG);
    Ok(())
}

fn load_to_minikube(image_name: &str) -> Result<()> {
    println!("📦 Loading {}:{} to minikube...", image_name, TAG);
    let tag_image = format!("{}:{}", image_name, TAG);

    let status = Command::new("minikube")
        .args(&["image", "load", &tag_image])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to load {} to minikube", image_name);
    }

    println!("✅ Loaded {}:{} to minikube", image_name, TAG);
    Ok(())
}

fn show_images() {
    println!("\n📦 Images in minikube:");
    let output = Command::new("minikube")
        .args(&["image", "ls"])
        .output()
        .unwrap();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.contains("hodei-jobs") {
            println!("  {}", line);
        }
    }
}

fn main() {
    let config = parse_args().unwrap();

    println!("╔═══════════════════════════════════════════╗");
    println!("║     HODEI JOBS - BUILD FOR MINIKUBE       ║");
    println!("╚═══════════════════════════════════════════╝");
    println!();
    println!("📦 Registry: {}", REGISTRY);
    println!("🏷️  Tag: {}", TAG);
    println!();

    let project_root = get_project_root();
    let dockerfile_server = project_root
        .join("Dockerfile.server")
        .to_str()
        .unwrap()
        .to_string();
    let dockerfile_worker = project_root
        .join("Dockerfile.worker")
        .to_str()
        .unwrap()
        .to_string();

    if config.build_server {
        build_image(
            &dockerfile_server,
            &format!("{}/hodei-jobs-server", REGISTRY),
            &project_root,
            config.no_cache,
        )
        .unwrap();
        load_to_minikube(&format!("{}/hodei-jobs-server", REGISTRY)).unwrap();
    }

    if config.build_worker {
        build_image(
            &dockerfile_worker,
            &format!("{}/hodei-jobs-worker", REGISTRY),
            &project_root,
            config.no_cache,
        )
        .unwrap();
        load_to_minikube(&format!("{}/hodei-jobs-worker", REGISTRY)).unwrap();
    }

    println!();
    println!("✅ Build complete!");
    show_images();

    println!("\n🚀 Deploy:");
    println!(
        r#"  helm upgrade --install hodei ./deploy/hodei-jobs-platform \
    -n hodei-jobs \
    -f ./deploy/hodei-jobs-platform/values-dev.yaml"#
    );
}
