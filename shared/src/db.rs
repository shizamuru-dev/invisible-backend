use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::info;

/// Initialize and return a PostgreSQL connection pool.
pub async fn init_postgres(config: &crate::config::AppConfig) -> Result<PgPool> {
    info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .context("Failed to connect to PostgreSQL database")?;

    info!("Successfully connected to PostgreSQL");
    Ok(pool)
}

/// Initialize and return a Redis connection manager.
pub async fn init_redis(
    config: &crate::config::AppConfig,
) -> Result<(redis::Client, ConnectionManager)> {
    info!("Connecting to Redis...");
    let client = redis::Client::open(config.redis_url.clone()).context("Invalid Redis URL")?;

    let manager = ConnectionManager::new(client.clone())
        .await
        .context("Failed to create Redis connection manager")?;

    info!("Successfully connected to Redis");
    Ok((client, manager))
}
