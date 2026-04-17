use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use shared::repository::{
    PgOfflineMessageRepository, RedisPresenceRepository, RedisPubSubRepository,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let config = match shared::config::AppConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "invisible_backend=debug,relay=debug,shared=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize databases
    let pg_pool = shared::db::init_postgres(&config).await?;
    let (redis_client, redis_manager) = shared::db::init_redis(&config).await?;

    tracing::info!("Running database migrations...");
    sqlx::migrate!("../migrations").run(&pg_pool).await?;

    info!("Database initialized");

    let offline_repo = Arc::new(PgOfflineMessageRepository::new(pg_pool.clone()));
    let pubsub_repo = Arc::new(RedisPubSubRepository::new(redis_manager.clone()));
    let presence_repo = Arc::new(RedisPresenceRepository::new(redis_manager));

    let app = relay::app(
        pg_pool,
        offline_repo,
        redis_client,
        pubsub_repo,
        presence_repo,
        config.jwt_secret,
    );

    let addr = format!("{}:{}", config.relay_host, config.relay_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Relay server listening on ws://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
