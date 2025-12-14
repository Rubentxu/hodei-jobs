// Binary principal para ejecutar el servidor Hodei
// Compile: cargo build --bin hodei-server
// Run: cargo run --bin hodei-server

use hodei_interface::api::HttpServer;
use tracing::{info, warn};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configurar logging
    tracing_subscriber::fmt::init();

    info!("🚀 Starting Hodei Job Platform Server");
    
    // Configurar dirección del servidor
    let addr: SocketAddr = "127.0.0.1:3000".parse()
        .map_err(|e| {
            warn!("Invalid address format: {}", e);
            e
        })?;

    // Crear y ejecutar servidor
    let server = HttpServer::new(addr);
    server.run().await?;

    Ok(())
}