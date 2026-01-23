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
    pub log_dir: std::path::PathBuf,
    /// Tiempo de espera máximo después de completar un job antes de auto-terminarse
    /// Valor 0 deshabilita la auto-terminación
    pub cleanup_timeout_ms: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        // Use tmpfs for logs if available (via Kubernetes hodei-tmp volume mount at /tmp)
        let log_dir = std::path::PathBuf::from("/tmp/hodei-logs");

        // Get server address from environment
        // Can be either:
        // - Full URL: "http://host:port" or "https://host:port"
        // - Just hostname: "host.domain" (will use default port 9090)
        let server_addr_raw =
            env::var("HODEI_SERVER_ADDRESS").unwrap_or_else(|_| "localhost:9090".to_string());

        // Extract hostname (remove protocol if present, remove port if present)
        let server_hostname = if server_addr_raw.starts_with("http://") {
            server_addr_raw
                .strip_prefix("http://")
                .unwrap_or(&server_addr_raw)
        } else if server_addr_raw.starts_with("https://") {
            server_addr_raw
                .strip_prefix("https://")
                .unwrap_or(&server_addr_raw)
        } else {
            &server_addr_raw
        };

        // Remove port if present
        let server_hostname: String = if let Some(colon_pos) = server_hostname.find(':') {
            server_hostname[..colon_pos].to_string()
        } else {
            server_hostname.to_string()
        };

        // Get protocol (default: http)
        let protocol = env::var("HODEI_SERVER_PROTOCOL").unwrap_or_else(|_| "http".to_string());

        // Get port (default: 9090)
        let port = env::var("HODEI_SERVER_PORT")
            .unwrap_or_else(|_| "9090".to_string())
            .parse()
            .unwrap_or(9090);

        // Compose final URL
        let server_addr = format!("{}://{}:{}", protocol, server_hostname, port);

        Self {
            worker_id: env::var("HODEI_WORKER_ID")
                .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()),
            worker_name: env::var("HODEI_WORKER_NAME")
                .unwrap_or_else(|_| format!("Worker Agent on {}", hostname)),
            server_addr,
            // Capabilities updated: NO DOCKER, only shell
            capabilities: vec!["shell".to_string()],
            cpu_cores: num_cpus::get() as f64,
            memory_bytes: get_system_memory(),
            disk_bytes: get_disk_space(),
            auth_token: env::var("HODEI_OTP_TOKEN").unwrap_or_else(|_| String::new()),
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
            log_dir: env::var("HODEI_LOG_DIR")
                .map(|p| std::path::PathBuf::from(p))
                .unwrap_or(log_dir),
            cleanup_timeout_ms: env::var("HODEI_CLEANUP_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30000), // Default 30 segundos, 0 para deshabilitar
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
