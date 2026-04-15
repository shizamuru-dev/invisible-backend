use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use shared::repository::{
    PgOfflineMessageRepository, RedisPresenceRepository, RedisPubSubRepository,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "invisible_backend=debug,relay=debug,shared=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize databases
    let pg_pool = shared::db::init_postgres().await?;
    let (redis_client, redis_manager) = shared::db::init_redis().await?;

    // Create table if it doesn't exist
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS offline_messages (
            id SERIAL PRIMARY KEY,
            to_user VARCHAR NOT NULL,
            payload JSONB NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );",
    )
    .execute(&pg_pool)
    .await?;

    info!("Database initialized");

    let offline_repo = Arc::new(PgOfflineMessageRepository::new(pg_pool.clone()));
    let pubsub_repo = Arc::new(RedisPubSubRepository::new(redis_manager.clone()));
    let presence_repo = Arc::new(RedisPresenceRepository::new(redis_manager));
    let jwt_secret =
        std::env::var("JWT_SECRET").unwrap_or_else(|_| "super-secret-key-for-dev".to_string());

    let app = relay::app(
        offline_repo,
        redis_client,
        pubsub_repo,
        presence_repo,
        jwt_secret,
    );

    let addr = "0.0.0.0:3030";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Relay server listening on ws://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
