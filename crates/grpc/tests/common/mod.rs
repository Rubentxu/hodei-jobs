use testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;
use uuid::Uuid;

use sqlx::{Connection, PgConnection};

struct SharedPostgresContext {
    _container: ContainerAsync<Postgres>,
    admin_connection_string: String,
    host: String,
    port: u16,
}

static POSTGRES_CONTEXT: OnceCell<SharedPostgresContext> = OnceCell::const_new();

pub struct PostgresTestDatabase {
    pub connection_string: String,
    db_name: String,
    admin_connection_string: String,
}

impl Drop for PostgresTestDatabase {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let admin_connection_string = self.admin_connection_string.clone();

        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };

        handle.spawn(async move {
            let Ok(mut conn) = PgConnection::connect(&admin_connection_string).await else {
                return;
            };

            let _ = sqlx::query(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid();",
            )
            .bind(&db_name)
            .execute(&mut conn)
            .await;

            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
                .execute(&mut conn)
                .await;
        });
    }
}

pub async fn get_postgres_context() -> anyhow::Result<PostgresTestDatabase> {
    let ctx = POSTGRES_CONTEXT
        .get_or_try_init(|| async {
            let container = Postgres::default()
                .with_tag("16-alpine")
                .start()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to start Postgres container: {e}"))?;

            let host = container
                .get_host()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get host: {e}"))?;
            let host = host.to_string();
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get port: {e}"))?;

            let admin_connection_string =
                format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

            Ok::<SharedPostgresContext, anyhow::Error>(SharedPostgresContext {
                _container: container,
                admin_connection_string,
                host,
                port,
            })
        })
        .await?;

    let db_suffix = Uuid::new_v4().to_string().replace('-', "");
    let db_name = format!("test_{}", db_suffix);

    let mut conn = PgConnection::connect(&ctx.admin_connection_string)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to postgres admin db: {e}"))?;

    sqlx::query(&format!("CREATE DATABASE {}", db_name))
        .execute(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create test database: {e}"))?;

    let connection_string =
        format!("postgres://postgres:postgres@{}:{}/{}", ctx.host, ctx.port, db_name);

    Ok(PostgresTestDatabase {
        connection_string,
        db_name,
        admin_connection_string: ctx.admin_connection_string.clone(),
    })
}
