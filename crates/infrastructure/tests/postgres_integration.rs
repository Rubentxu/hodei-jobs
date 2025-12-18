//! Integration tests for PostgresProviderConfigRepository
//!
//! Uses TestContainers for PostgreSQL. Pattern: Single Instance + Resource Pooling.

use hodei_jobs_domain::{
    providers::config::{
        DockerConfig, ProviderConfig, ProviderConfigRepository, ProviderTypeConfig,
    },
    shared_kernel::ProviderStatus,
    workers::ProviderType,
};
use hodei_jobs_infrastructure::persistence::{DatabaseConfig, PostgresProviderConfigRepository};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

/// Container info for resource pooling
struct PostgresTestContext {
    _container: ContainerAsync<Postgres>,
    connection_string: String,
}

/// Global Postgres container instance (Single Instance pattern)
static POSTGRES_CONTEXT: OnceCell<PostgresTestContext> = OnceCell::const_new();

/// Get or create the shared PostgreSQL container
async fn get_postgres_context() -> &'static PostgresTestContext {
    POSTGRES_CONTEXT
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("16-alpine")
                .start()
                .await
                .expect("Failed to start Postgres container");

            let host = container.get_host().await.expect("Failed to get host");
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .expect("Failed to get port");

            let connection_string =
                format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

            PostgresTestContext {
                _container: container,
                connection_string,
            }
        })
        .await
}

/// Create a repository connected to the shared container
async fn create_test_repository() -> PostgresProviderConfigRepository {
    let ctx = get_postgres_context().await;

    let config = DatabaseConfig {
        url: ctx.connection_string.clone(),
        max_connections: 5,
        connection_timeout: std::time::Duration::from_secs(30),
    };

    let repo = PostgresProviderConfigRepository::connect(&config)
        .await
        .expect("Failed to connect to Postgres");

    repo.run_migrations()
        .await
        .expect("Failed to run migrations");

    repo
}

/// Create a test ProviderConfig
fn create_test_config(name: &str) -> ProviderConfig {
    ProviderConfig::new(
        name.to_string(),
        ProviderType::Docker,
        ProviderTypeConfig::Docker(DockerConfig::default()),
    )
}

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_save_and_find() {
    let repo = create_test_repository().await;
    let config = create_test_config("pg-test-save-find");

    // Save
    let save_result = repo.save(&config).await;
    assert!(save_result.is_ok(), "Save failed: {:?}", save_result.err());

    // Find by ID
    let found = repo.find_by_id(&config.id).await;
    assert!(found.is_ok());
    let found = found.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, config.name);

    // Cleanup
    let _ = repo.delete(&config.id).await;
}

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_find_by_name() {
    let repo = create_test_repository().await;
    let config = create_test_config("pg-test-find-name");

    repo.save(&config).await.expect("Save failed");

    let found = repo.find_by_name(&config.name).await;
    assert!(found.is_ok());
    assert!(found.unwrap().is_some());

    // Cleanup
    let _ = repo.delete(&config.id).await;
}

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_find_by_type() {
    let repo = create_test_repository().await;
    let config = create_test_config("pg-test-find-type");

    repo.save(&config).await.expect("Save failed");

    let found = repo.find_by_type(&ProviderType::Docker).await;
    assert!(found.is_ok());
    let configs = found.unwrap();
    assert!(configs.iter().any(|c| c.name == config.name));

    // Cleanup
    let _ = repo.delete(&config.id).await;
}

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_find_enabled() {
    let repo = create_test_repository().await;
    let config = create_test_config("pg-test-find-enabled");

    repo.save(&config).await.expect("Save failed");

    let found = repo.find_enabled().await;
    assert!(found.is_ok());
    let configs = found.unwrap();
    assert!(configs.iter().any(|c| c.name == config.name));

    // Cleanup
    let _ = repo.delete(&config.id).await;
}

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_update() {
    let repo = create_test_repository().await;
    let mut config = create_test_config("pg-test-update");

    repo.save(&config).await.expect("Save failed");

    // Update
    config.priority = 999;
    config.status = ProviderStatus::Disabled;
    let update_result = repo.update(&config).await;
    assert!(update_result.is_ok());

    // Verify
    let found = repo.find_by_id(&config.id).await.unwrap().unwrap();
    assert_eq!(found.priority, 999);
    assert_eq!(found.status, ProviderStatus::Disabled);

    // Cleanup
    let _ = repo.delete(&config.id).await;
}

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_delete() {
    let repo = create_test_repository().await;
    let config = create_test_config("pg-test-delete");

    repo.save(&config).await.expect("Save failed");

    // Delete
    let delete_result = repo.delete(&config.id).await;
    assert!(delete_result.is_ok());

    // Verify deleted
    let found = repo.find_by_id(&config.id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_exists_by_name() {
    let repo = create_test_repository().await;
    let config = create_test_config("pg-test-exists");

    repo.save(&config).await.expect("Save failed");

    let exists = repo.exists_by_name(&config.name).await;
    assert!(exists.is_ok());
    assert!(exists.unwrap());

    let not_exists = repo.exists_by_name("non-existent-provider").await;
    assert!(not_exists.is_ok());
    assert!(!not_exists.unwrap());

    // Cleanup
    let _ = repo.delete(&config.id).await;
}
