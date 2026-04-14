use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "invisible_backend=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = relay::app();

    let addr = "0.0.0.0:3030";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Relay server listening on ws://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
