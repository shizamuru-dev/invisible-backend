use anyhow::Result;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{FromRef, FromRequestParts, Path, Query, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use s3::Bucket;
use s3::Region;
use s3::creds::Credentials;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
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

#[derive(Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    token: String,
}

use shared::models::Claims;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    jwt_secret: String,
}

pub struct AuthenticatedUser(pub Claims);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: axum::extract::FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|val| val.to_str().ok())
            .and_then(|val| val.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        let token = if let Some(t) = auth_header {
            t
        } else {
            // Fallback to token query parameter
            let query = parts.uri.query().unwrap_or("");
            let mut token_from_query = None;
            for pair in query.split('&') {
                let mut parts = pair.split('=');
                if parts.next() == Some("token") {
                    if let Some(t) = parts.next() {
                        token_from_query = Some(t.to_string());
                        break;
                    }
                }
            }
            token_from_query.ok_or((StatusCode::UNAUTHORIZED, "Missing authorization token"))?
        };

        let token_data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(app_state.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid or expired token"))?;

        Ok(AuthenticatedUser(token_data.claims))
    }
}

fn get_s3_bucket() -> Bucket {
    let endpoint =
        std::env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let access_key = std::env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let secret_key = std::env::var("S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let region_name = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());

    let creds = Credentials::new(Some(&access_key), Some(&secret_key), None, None, None).unwrap();
    let region = Region::Custom {
        region: region_name,
        endpoint,
    };

    *Bucket::new("uploads", region, creds)
        .unwrap()
        .with_path_style()
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

    info!("Connecting to PostgreSQL...");
    let db = shared::db::init_postgres().await?;

    // Create users table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            username VARCHAR(255) UNIQUE NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
        );",
    )
    .execute(&db)
    .await?;

    let jwt_secret =
        std::env::var("JWT_SECRET").unwrap_or_else(|_| "super-secret-key-for-dev".to_string());

    let state = AppState { db, jwt_secret };

    let app = Router::new()
        .route("/health", get(|| async { "Auth server is alive" }))
        .route("/api/auth/register", post(register_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/files/presign", get(presign_handler))
        .route("/files/download/{file_id}", get(download_handler))
        .with_state(state);

    let addr = "0.0.0.0:3001";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Auth API server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> impl IntoResponse {
    if payload.username.is_empty() || payload.password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "Username and password are required",
        )
            .into_response();
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = match argon2.hash_password(payload.password.as_bytes(), &salt) {
        Ok(hash) => hash.to_string(),
        Err(e) => {
            error!("Failed to hash password: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let result = sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, $2)")
        .bind(payload.username)
        .bind(password_hash)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => (StatusCode::CREATED, "User created successfully").into_response(),
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            (StatusCode::CONFLICT, "Username already exists").into_response()
        }
        Err(e) => {
            error!("Database error during registration: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}

async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> impl IntoResponse {
    use sqlx::Row;
    let user = sqlx::query("SELECT id, password_hash FROM users WHERE username = $1")
        .bind(payload.username.clone())
        .fetch_optional(&state.db)
        .await;

    let user_row = match user {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, "Invalid username or password").into_response();
        }
        Err(e) => {
            error!("Database error during login: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let password_hash: String = user_row.get("password_hash");

    let parsed_hash = match PasswordHash::new(&password_hash) {
        Ok(hash) => hash,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    if Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return (StatusCode::UNAUTHORIZED, "Invalid username or password").into_response();
    }

    let expiration = Utc::now()
        .checked_add_signed(Duration::days(7))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: payload.username.clone(),
        exp: expiration as usize,
    };

    let token = match encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to create token: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    (StatusCode::OK, Json(AuthResponse { token })).into_response()
}

async fn presign_handler(
    _user: AuthenticatedUser,
    Query(_params): Query<PresignQuery>,
) -> impl IntoResponse {
    let bucket = get_s3_bucket();

    // Generate a unique ID for the file
    let file_id = Uuid::new_v4().to_string();
    let object_path = format!("/{}", file_id);

    // Presigned PUT URL valid for 5 minutes (300 seconds)
    let upload_url = bucket
        .presign_put(object_path, 300, None, None)
        .await
        .unwrap();

    // Secure download URL points back to our auth server
    let api_url = std::env::var("API_URL").unwrap_or_else(|_| "http://localhost:3001".to_string());
    let download_url = format!("{}/files/download/{}", api_url, file_id);

    Json(PresignResponse {
        upload_url,
        download_url,
        file_id,
    })
}

async fn download_handler(
    _user: AuthenticatedUser,
    Path(file_id): Path<String>,
) -> impl IntoResponse {
    let bucket = get_s3_bucket();
    let object_path = format!("/{}", file_id);

    // Generate presigned GET URL (valid for 5 mins)
    match bucket.presign_get(object_path, 300, None).await {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(e) => {
            error!("Failed to generate presigned GET url: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate download link",
            )
                .into_response()
        }
    }
}
