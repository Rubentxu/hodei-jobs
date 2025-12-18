use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_server_port")]
    pub port: u16,
    pub database_url: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_server_port() -> u16 {
    50051
}

fn default_log_level() -> String {
    "info".to_string()
}

impl ServerConfig {
    pub fn new() -> Result<Self, config::ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = config::Config::builder()
            // Start with default values
            .set_default("port", 50051)?
            .set_default("log_level", "info")?
            // Merge with config file (if exists)
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name(&format!("config/{}", run_mode)).required(false))
            // Merge with environment variables (SERVER_...)
            .add_source(config::Environment::with_prefix("SERVER"))
            .build()?;

        s.try_deserialize()
    }
}
