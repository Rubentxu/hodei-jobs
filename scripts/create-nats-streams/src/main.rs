use async_nats::{jetstream, ConnectOptions};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let nats_url = std::env::args().nth(1).unwrap_or_else(|| "nats://10.43.247.182:4222".to_string());

    println!("Conectando a NATS: {}", nats_url);

    let nats_client = async_nats::connect(nats_url).await?;
    let jetstream = jetstream::new(nats_client.clone());

    // Crear stream HODEI_COMMANDS
    let stream_config = async_nats::jetstream::stream::Config {
        name: "HODEI_COMMANDS".to_string(),
        subjects: vec!["hodei.commands.>".to_string()],
        description: Some("Persistent stream for Hodei commands".to_string()),
        max_messages: Some(1_000_000),
        max_bytes: Some(1024 * 1024 * 1024), // 1GB
        storage: async_nats::jetstream::stream::StorageType::File,
        ..Default::default()
    };

    match jetstream.get_stream("HODEI_COMMANDS").await {
        Ok(stream) => {
            println!("Stream HODEI_COMMANDS ya existe");
            println!("  Mensajes: {}", stream.info().await?.messages);
        }
        Err(_) => {
            println!("Creando stream HODEI_COMMANDS...");
            jetstream.create_stream(stream_config).await?;
            println!("✅ Stream HODEI_COMMANDS creado");
        }
    }

    // Crear stream SAGA_TASKS si no existe
    let saga_config = async_nats::jetstream::stream::Config {
        name: "SAGA_TASKS".to_string(),
        subjects: vec!["saga.tasks.>".to_string()],
        description: Some("Persistent stream for saga tasks".to_string()),
        max_messages: Some(500_000),
        max_bytes: Some(512 * 1024 * 512), // 512MB
        storage: async_nats::jetstream::stream::StorageType::File,
        ..Default::default()
    };

    match jetstream.get_stream("SAGA_TASKS").await {
        Ok(stream) => {
            println!("Stream SAGA_TASKS ya existe");
            println!("  Mensajes: {}", stream.info().await?.messages);
        }
        Err(_) => {
            println!("Creando stream SAGA_TASKS...");
            jetstream.create_stream(saga_config).await?;
            println!("✅ Stream SAGA_TASKS creado");
        }
    }

    // Crear stream HODEI_EVENTS si no existe
    let events_config = async_nats::jetstream::stream::Config {
        name: "HODEI_EVENTS".to_string(),
        subjects: vec!["hodei.events.>".to_string()],
        description: Some("Persistent stream for Hodei events".to_string()),
        max_messages: Some(2_000_000),
        max_bytes: Some(2 * 1024 * 1024 * 1024), // 2GB
        storage: async_nats::jetstream::stream::StorageType::File,
        ..Default::default()
    };

    match jetstream.get_stream("HODEI_EVENTS").await {
        Ok(stream) => {
            println!("Stream HODEI_EVENTS ya existe");
            println!("  Mensajes: {}", stream.info().await?.messages);
        }
        Err(_) => {
            println!("Creando stream HODEI_EVENTS...");
            jetstream.create_stream(events_config).await?;
            println!("✅ Stream HODEI_EVENTS creado");
        }
    }

    println!("\n🎉 Streams verificados/creados correctamente");

    // Keep connection alive briefly
    tokio::time::sleep(Duration::from_secs(1)).await;

    Ok(())
}
