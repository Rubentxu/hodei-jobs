use hodei_server_domain::providers::config::ProviderConfigRepository;
use hodei_server_domain::providers::config::{DockerConfig, ProviderConfig, ProviderTypeConfig};
use hodei_server_domain::shared_kernel::{ProviderId, ProviderStatus};
use hodei_server_domain::workers::ProviderType;
use hodei_server_infrastructure::persistence::{DatabaseConfig, PostgresProviderConfigRepository};

mod common;

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_provider_config_repository_lifecycle() {
    let db = common::get_postgres_context().await;

    let config = DatabaseConfig {
        url: db.connection_string.clone(),
        max_connections: 5,
        connection_timeout: std::time::Duration::from_secs(30),
    };

    let repo = PostgresProviderConfigRepository::connect(&config)
        .await
        .expect("Failed to connect to Postgres");

    repo.run_migrations()
        .await
        .expect("Failed to run migrations");

    let provider = ProviderConfig::new(
        "docker-local".to_string(),
        ProviderType::Docker,
        ProviderTypeConfig::Docker(DockerConfig::default()),
    )
    .with_priority(10)
    .with_max_workers(3)
    .with_tag("prod");

    repo.save(&provider).await.expect("Failed to save provider");

    let found = repo
        .find_by_id(&provider.id)
        .await
        .expect("Failed to find provider");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, provider.id);
    assert_eq!(found.name, "docker-local");

    let by_name = repo
        .find_by_name("docker-local")
        .await
        .expect("Failed to find provider by name");
    assert!(by_name.is_some());

    let enabled = repo.find_enabled().await.expect("Failed to find enabled");
    assert!(enabled.iter().any(|p| p.id == provider.id));

    let by_type = repo
        .find_by_type(&ProviderType::Docker)
        .await
        .expect("Failed to find by type");
    assert!(by_type.iter().any(|p| p.id == provider.id));

    let with_capacity = repo
        .find_with_capacity()
        .await
        .expect("Failed to find with capacity");
    assert!(with_capacity.iter().any(|p| p.id == provider.id));

    let mut updated = found.clone();
    updated.status = ProviderStatus::Disabled;
    updated.updated_at = chrono::Utc::now();

    repo.update(&updated)
        .await
        .expect("Failed to update provider");

    let found_updated = repo
        .find_by_id(&provider.id)
        .await
        .expect("Failed to find updated provider")
        .unwrap();
    assert_eq!(found_updated.status, ProviderStatus::Disabled);

    let exists = repo
        .exists_by_name("docker-local")
        .await
        .expect("Failed to check exists_by_name");
    assert!(exists);

    repo.delete(&provider.id)
        .await
        .expect("Failed to delete provider");

    let deleted = repo
        .find_by_id(&provider.id)
        .await
        .expect("Failed to find deleted provider");
    assert!(deleted.is_none());

    let _unused: ProviderId = provider.id;
}
