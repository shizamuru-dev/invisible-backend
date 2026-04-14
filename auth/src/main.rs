use anyhow::Result;
use axum::{
    extract::Query,
    response::IntoResponse,
    routing::get,
    Router, Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use s3::Bucket;
use s3::creds::Credentials;
use s3::Region;
use uuid::Uuid;

#[derive(Deserialize)]
struct PresignQuery {
    #[allow(dead_code)]
    file_name: String,
    #[allow(dead_code)]
    mime_type: String,
}

#[derive(Serialize)]
struct PresignResponse {
    upload_url: String,
    download_url: String,
    file_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "auth=debug,invisible_backend=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/health", get(|| async { "Auth server is alive" }))
        .route("/files/presign", get(presign_handler));

    let addr = "0.0.0.0:3001";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Auth API server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn presign_handler(Query(_params): Query<PresignQuery>) -> impl IntoResponse {
    let endpoint = std::env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let access_key = std::env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let secret_key = std::env::var("S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let region_name = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    
    let creds = Credentials::new(Some(&access_key), Some(&secret_key), None, None, None).unwrap();
    let region = Region::Custom {
        region: region_name,
        endpoint: endpoint.clone(),
    };
    
    let bucket = Bucket::new("uploads", region, creds).unwrap().with_path_style();
    
    // Generate a unique ID for the file
    let file_id = Uuid::new_v4().to_string();
    let object_path = format!("/{}", file_id);
    
    // Presigned PUT URL valid for 5 minutes (300 seconds)
    // NOTE: For MVP, custom content-type headers aren't strictly required in the signature if MinIO allows it
    let upload_url = bucket.presign_put(object_path, 300, None, None).await.unwrap();
    
    // Since the bucket is public, the download URL is just the direct URL
    // Public download URL format: http://localhost:9000/uploads/file_id
    // Warning: If testing with Docker Desktop, the client should hit localhost:9000 instead of `minio:9000`.
    let public_endpoint = std::env::var("S3_PUBLIC_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let download_url = format!("{}/uploads/{}", public_endpoint, file_id);

    Json(PresignResponse {
        upload_url,
        download_url,
        file_id,
    })
}