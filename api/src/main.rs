use anyhow::Result;
use api::{AppState, create_router, worker::start_database_worker};
use shared::config::AppConfig;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::load().unwrap();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api=debug,invisible_backend=debug,shared=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Connecting to PostgreSQL...");
    let db = shared::db::init_postgres(&config).await?;

    info!("Connecting to Redis...");
    let redis_client = redis::Client::open(config.redis_url.clone())?;

    info!("Running database migrations...");
    sqlx::migrate!("../migrations").run(&db).await?;

    let worker_db = db.clone();
    let worker_redis = redis_client.clone();
    tokio::spawn(async move {
        start_database_worker(worker_db, worker_redis).await;
    });

    let state = AppState {
        db,
        jwt_secret: config.jwt_secret.clone(),
        config: config.clone(),
        redis_client: redis_client.clone(),
    };

    let app = create_router(state);

    let addr = format!("{}:{}", config.api_host, config.api_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("API server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
