use std::env;

use hodei_worker_infrastructure::metrics::{get_disk_space, get_system_memory};

#[derive(Clone)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub worker_name: String,
    pub server_addr: String,
    pub capabilities: Vec<String>,
    pub cpu_cores: f64,
    pub memory_bytes: i64,
    pub disk_bytes: i64,
    pub auth_token: String,
    pub client_cert_path: Option<std::path::PathBuf>,
    pub client_key_path: Option<std::path::PathBuf>,
    pub ca_cert_path: Option<std::path::PathBuf>,
    pub log_batch_size: usize,
    pub log_flush_interval_ms: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        Self {
            worker_id: env::var("HODEI_WORKER_ID")
                .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()),
            worker_name: env::var("HODEI_WORKER_NAME")
                .unwrap_or_else(|_| format!("Worker Agent on {}", hostname)),
            server_addr: env::var("HODEI_SERVER_ADDR")
                .unwrap_or_else(|_| "http://localhost:50051".to_string()),
            // Capabilities updated: NO DOCKER, only shell
            capabilities: vec!["shell".to_string()],
            cpu_cores: num_cpus::get() as f64,
            memory_bytes: get_system_memory(),
            disk_bytes: get_disk_space(),
            auth_token: env::var("HODEI_AUTH_TOKEN").unwrap_or_else(|_| String::new()),
            client_cert_path: env::var("HODEI_CLIENT_CERT_PATH")
                .map(|p| std::path::PathBuf::from(p))
                .ok(),
            client_key_path: env::var("HODEI_CLIENT_KEY_PATH")
                .map(|p| std::path::PathBuf::from(p))
                .ok(),
            ca_cert_path: env::var("HODEI_CA_CERT_PATH")
                .map(|p| std::path::PathBuf::from(p))
                .ok(),
            log_batch_size: env::var("HODEI_LOG_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            log_flush_interval_ms: env::var("HODEI_LOG_FLUSH_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(250),
        }
    }
}

impl WorkerConfig {
    pub fn is_mtls_enabled(&self) -> bool {
        self.client_cert_path.is_some()
            && self.client_key_path.is_some()
            && self.ca_cert_path.is_some()
    }
}
