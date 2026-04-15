use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::info;

/// Initialize and return a PostgreSQL connection pool.
///
/// Requires the `DATABASE_URL` environment variable to be set.
pub async fn init_postgres() -> Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://invisible:password@127.0.0.1:5432/invisible_chat".to_string()
    });

    info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to PostgreSQL database")?;

    info!("Successfully connected to PostgreSQL");
    Ok(pool)
}

/// Initialize and return a Redis connection manager.
///
/// Requires the `REDIS_URL` environment variable to be set.
pub async fn init_redis() -> Result<(redis::Client, ConnectionManager)> {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());

    info!("Connecting to Redis...");
    let client = redis::Client::open(redis_url).context("Invalid Redis URL")?;

    let manager = ConnectionManager::new(client.clone())
        .await
        .context("Failed to create Redis connection manager")?;

    info!("Successfully connected to Redis");
    Ok((client, manager))
}
