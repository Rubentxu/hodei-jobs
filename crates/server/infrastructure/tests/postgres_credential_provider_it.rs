//! Integration tests for PostgresCredentialProvider
//!
//! These tests require a running PostgreSQL instance (via Testcontainers).
//! Run with: cargo test --test postgres_credential_provider_it -- --ignored

use hodei_server_domain::credentials::{
    AuditAction, AuditContext, CredentialProvider, SecretValue,
};
use hodei_server_infrastructure::credentials::{
    CredentialDatabaseConfig, EncryptionKey, PostgresCredentialProvider,
};
use sqlx::{Connection, PgConnection};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;

/// Creates a test database and returns connection string
async fn setup_test_database() -> (testcontainers::ContainerAsync<Postgres>, String) {
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

    let connection_string = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

    // Wait for database to be ready
    for _ in 0..30 {
        if PgConnection::connect(&connection_string).await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    (container, connection_string)
}

/// Creates a provider with test configuration
async fn create_test_provider(connection_string: &str) -> PostgresCredentialProvider {
    let config = CredentialDatabaseConfig::new(connection_string.to_string());
    let encryption_key = EncryptionKey::generate();

    let provider = PostgresCredentialProvider::connect(&config, encryption_key)
        .await
        .expect("Failed to connect to database");

    provider
        .run_migrations()
        .await
        .expect("Failed to run migrations");

    provider
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_set_and_get_secret() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    // Set a secret
    let secret = provider
        .set_secret("test_key", SecretValue::from_str("secret_value"))
        .await
        .expect("Failed to set secret");

    assert_eq!(secret.key(), "test_key");
    assert_eq!(secret.version(), 1);
    assert_eq!(secret.value().expose_str(), Some("secret_value"));

    // Get the secret
    let retrieved = provider
        .get_secret("test_key")
        .await
        .expect("Failed to get secret");

    assert_eq!(retrieved.key(), "test_key");
    assert_eq!(retrieved.value().expose_str(), Some("secret_value"));
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_update_secret_increments_version() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    // Set initial secret
    let secret1 = provider
        .set_secret("versioned_key", SecretValue::from_str("value_v1"))
        .await
        .expect("Failed to set secret");
    assert_eq!(secret1.version(), 1);

    // Update secret
    let secret2 = provider
        .set_secret("versioned_key", SecretValue::from_str("value_v2"))
        .await
        .expect("Failed to update secret");
    assert_eq!(secret2.version(), 2);
    assert_eq!(secret2.value().expose_str(), Some("value_v2"));

    // Third update
    let secret3 = provider
        .set_secret("versioned_key", SecretValue::from_str("value_v3"))
        .await
        .expect("Failed to update secret");
    assert_eq!(secret3.version(), 3);
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_get_nonexistent_secret_returns_not_found() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    let result = provider.get_secret("nonexistent_key").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_delete_secret() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    // Set a secret
    provider
        .set_secret("to_delete", SecretValue::from_str("temporary"))
        .await
        .expect("Failed to set secret");

    // Verify it exists
    assert!(provider.get_secret("to_delete").await.is_ok());

    // Delete it
    provider
        .delete_secret("to_delete")
        .await
        .expect("Failed to delete secret");

    // Verify it's gone
    let result = provider.get_secret("to_delete").await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_delete_nonexistent_secret_returns_not_found() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    let result = provider.delete_secret("nonexistent").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_list_secrets() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    // Create multiple secrets
    provider
        .set_secret("app/db/password", SecretValue::from_str("pw1"))
        .await
        .unwrap();
    provider
        .set_secret("app/db/username", SecretValue::from_str("user1"))
        .await
        .unwrap();
    provider
        .set_secret("app/api/key", SecretValue::from_str("key1"))
        .await
        .unwrap();
    provider
        .set_secret("other/secret", SecretValue::from_str("other"))
        .await
        .unwrap();

    // List all secrets
    let all_keys = provider.list_secrets(None).await.expect("Failed to list");
    assert_eq!(all_keys.len(), 4);

    // List with prefix
    let db_keys = provider
        .list_secrets(Some("app/db/"))
        .await
        .expect("Failed to list with prefix");
    assert_eq!(db_keys.len(), 2);
    assert!(db_keys.contains(&"app/db/password".to_string()));
    assert!(db_keys.contains(&"app/db/username".to_string()));

    // List with different prefix
    let api_keys = provider
        .list_secrets(Some("app/api/"))
        .await
        .expect("Failed to list");
    assert_eq!(api_keys.len(), 1);
    assert!(api_keys.contains(&"app/api/key".to_string()));
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_health_check() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    let result = provider.health_check().await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_encryption_is_transparent() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    let secret_data = "super_secret_password_123!@#$%^";

    provider
        .set_secret("encrypted_secret", SecretValue::from_str(secret_data))
        .await
        .expect("Failed to set secret");

    let retrieved = provider
        .get_secret("encrypted_secret")
        .await
        .expect("Failed to get secret");

    // Decrypted value should match original
    assert_eq!(retrieved.value().expose_str(), Some(secret_data));
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_binary_secret_data() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    // Binary data with null bytes
    let binary_data: Vec<u8> = vec![0x00, 0x01, 0xFF, 0xFE, 0x42, 0x00, 0xAB];

    provider
        .set_secret("binary_secret", SecretValue::new(binary_data.clone()))
        .await
        .expect("Failed to set binary secret");

    let retrieved = provider
        .get_secret("binary_secret")
        .await
        .expect("Failed to get binary secret");

    assert_eq!(retrieved.value().expose(), binary_data.as_slice());
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_audit_access_logs_to_database() {
    let (_container, connection_string) = setup_test_database().await;
    let provider = create_test_provider(&connection_string).await;

    let context = AuditContext::new("test-user")
        .with_correlation_id("req-123")
        .with_ip_address("192.168.1.1");

    // Log an audit event
    provider
        .audit_access("some_key", AuditAction::Read, &context)
        .await;

    // Verify the audit log was written (query the database directly)
    let mut conn = PgConnection::connect(&connection_string)
        .await
        .expect("Failed to connect");

    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM secret_access_log WHERE secret_key = $1")
            .bind("some_key")
            .fetch_one(&mut conn)
            .await
            .expect("Failed to query audit log");

    assert_eq!(row.0, 1, "Expected exactly one audit log entry");
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_different_encryption_keys_cannot_decrypt() {
    let (_container, connection_string) = setup_test_database().await;

    // Create provider 1 with key A
    let config = CredentialDatabaseConfig::new(connection_string.clone());
    let key_a = EncryptionKey::generate();
    let provider_a = PostgresCredentialProvider::connect(&config, key_a)
        .await
        .expect("Failed to connect");
    provider_a
        .run_migrations()
        .await
        .expect("Failed to migrate");

    // Store a secret with provider A
    provider_a
        .set_secret("encrypted_by_a", SecretValue::from_str("secret_a"))
        .await
        .expect("Failed to set secret");

    // Create provider 2 with different key B (same database)
    let key_b = EncryptionKey::generate();
    let provider_b = PostgresCredentialProvider::connect(&config, key_b)
        .await
        .expect("Failed to connect");

    // Try to read the secret with provider B (should fail decryption)
    let result = provider_b.get_secret("encrypted_by_a").await;
    assert!(result.is_err(), "Should fail to decrypt with wrong key");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Encryption") || err.to_string().contains("decrypt"),
        "Error should mention encryption issue"
    );
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers"]
async fn test_provider_name() {
    assert_eq!(PostgresCredentialProvider::provider_name(), "postgres");
}
