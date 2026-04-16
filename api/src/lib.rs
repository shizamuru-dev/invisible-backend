pub mod e2ee;
pub mod worker;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{FromRef, FromRequestParts, Path, Query, State},
    http::{StatusCode, request::Parts},
    response::IntoResponse,
    routing::{delete, get, post},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, warn};

const USERNAME_MIN_LEN: usize = 3;
const USERNAME_MAX_LEN: usize = 32;
const PASSWORD_MIN_LEN: usize = 8;

use uuid::Uuid;

pub use shared::models::Claims;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: String,
    pub redis_client: redis::Client,
}

pub struct AuthenticatedUser(pub Claims);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: FromRef<S>,
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
            let query = parts.uri.query().unwrap_or("");
            let mut token_from_query = None;
            for pair in query.split('&') {
                let mut parts_iter = pair.split('=');
                if parts_iter.next() == Some("token")
                    && let Some(t) = parts_iter.next()
                {
                    token_from_query = Some(t.to_string());
                    break;
                }
            }
            if token_from_query.is_some() {
                warn!(
                    "Using URL token authentication - consider using Authorization header for production"
                );
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

#[derive(Deserialize)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_info: Option<DeviceInfoPayload>,
}

#[derive(Deserialize)]
pub struct DeviceInfoPayload {
    pub device_name: String,
    pub device_model: String,
    pub platform: String,
    pub hwid: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "API server is alive" }))
        .route("/api/auth/register", post(register_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/logout", post(logout_handler))
        .route("/keys/upload", post(e2ee::upload_keys))
        .route("/keys/claim/{user_id}", get(e2ee::claim_keys))
        .route("/keys/devices", get(e2ee::list_devices))
        .route("/keys/devices/{device_id}", delete(e2ee::delete_device))
        .route("/files/presign", get(presign_handler))
        .route("/files/download/{file_id}", get(download_handler))
        .with_state(state)
}

async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> impl IntoResponse {
    let username = payload.username.trim();
    if username.len() < USERNAME_MIN_LEN || username.len() > USERNAME_MAX_LEN {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "Username must be {} to {} characters",
                USERNAME_MIN_LEN, USERNAME_MAX_LEN
            ),
        )
            .into_response();
    }
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return (
            StatusCode::BAD_REQUEST,
            "Username can only contain letters, numbers, and underscores",
        )
            .into_response();
    }
    if payload.password.len() < PASSWORD_MIN_LEN {
        return (
            StatusCode::BAD_REQUEST,
            format!("Password must be at least {} characters", PASSWORD_MIN_LEN),
        )
            .into_response();
    }
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

    let session_id = Uuid::new_v4().to_string();

    let insert_result = if let Some(ref info) = payload.device_info {
        sqlx::query(
            "INSERT INTO sessions (id, user_username, refresh_token, is_valid, device_name, device_model, platform, hwid) VALUES ($1, $2, $3, true, $4, $5, $6, $7)",
        )
        .bind(Uuid::parse_str(&session_id).unwrap())
        .bind(payload.username.clone())
        .bind(Uuid::new_v4().to_string())
        .bind(&info.device_name)
        .bind(&info.device_model)
        .bind(&info.platform)
        .bind(&info.hwid)
        .execute(&state.db)
        .await
    } else {
        sqlx::query(
            "INSERT INTO sessions (id, user_username, refresh_token, is_valid) VALUES ($1, $2, $3, true)",
        )
        .bind(Uuid::parse_str(&session_id).unwrap())
        .bind(payload.username.clone())
        .bind(Uuid::new_v4().to_string())
        .execute(&state.db)
        .await
    };

    if let Err(e) = insert_result {
        error!("Failed to create session: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
    }

    if let Ok(mut conn) = state.redis_client.get_multiplexed_async_connection().await {
        use redis::AsyncCommands;
        let session_key = format!("session:{}", session_id);
        let _: redis::RedisResult<()> = conn.set_ex(session_key, "valid", 30 * 24 * 60 * 60).await;
    }

    let expiration = Utc::now()
        .checked_add_signed(Duration::days(7))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: payload.username.clone(),
        session_id: session_id.clone(),
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

async fn presign_handler(
    _user: AuthenticatedUser,
    Query(_params): Query<PresignQuery>,
) -> impl IntoResponse {
    let file_id = Uuid::new_v4().to_string();
    Json(PresignResponse {
        upload_url: format!("http://localhost:9000/{}", file_id),
        download_url: format!("http://localhost:3001/files/download/{}", file_id),
        file_id,
    })
}

async fn download_handler(
    _user: AuthenticatedUser,
    Path(file_id): Path<String>,
) -> impl IntoResponse {
    (StatusCode::OK, format!("file: {}", file_id))
}

pub async fn logout_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> impl IntoResponse {
    let session_id = user.0.session_id;

    if let Ok(uuid) = Uuid::parse_str(&session_id) {
        let _ = sqlx::query("UPDATE sessions SET is_valid = false WHERE id = $1")
            .bind(uuid)
            .execute(&state.db)
            .await;
    }

    if let Ok(mut conn) = state.redis_client.get_multiplexed_async_connection().await {
        use redis::AsyncCommands;
        let session_key = format!("session:{}", session_id);
        let _: redis::RedisResult<()> = conn.del(session_key).await;
    }

    (StatusCode::OK, "Logged out successfully").into_response()
}
