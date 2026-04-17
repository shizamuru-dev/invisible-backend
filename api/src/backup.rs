use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{AppState, AuthenticatedUser};

#[derive(Deserialize, Serialize, Debug)]
pub struct BackupVaultRequest {
    pub encrypted_vault: String,
    pub salt: String,
    pub mac: String,
}

pub async fn upload_backup(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<BackupVaultRequest>,
) -> impl IntoResponse {
    let username = &user.0.sub;

    let result = sqlx::query(
        r#"
        INSERT INTO user_key_backups (user_username, encrypted_vault, salt, mac, updated_at)
        VALUES ($1, $2, $3, $4, NOW())
        ON CONFLICT (user_username)
        DO UPDATE SET encrypted_vault = EXCLUDED.encrypted_vault, salt = EXCLUDED.salt, mac = EXCLUDED.mac, updated_at = NOW()
        "#
    )
    .bind(username)
    .bind(&payload.encrypted_vault)
    .bind(&payload.salt)
    .bind(&payload.mac)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            info!("User {} updated key backup vault", username);
            (StatusCode::OK, "Backup saved").into_response()
        }
        Err(e) => {
            error!("Failed to save key backup: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

#[derive(Serialize)]
pub struct BackupVaultResponse {
    pub encrypted_vault: String,
    pub salt: String,
    pub mac: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn get_backup(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> impl IntoResponse {
    let username = &user.0.sub;

    #[derive(Debug, sqlx::FromRow)]
    struct BackupRow {
        encrypted_vault: String,
        salt: String,
        mac: String,
        updated_at: chrono::DateTime<chrono::Utc>,
    }

    let result = sqlx::query_as::<_, BackupRow>(
        "SELECT encrypted_vault, salt, mac, updated_at FROM user_key_backups WHERE user_username = $1"
    )
    .bind(username)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(row)) => Json(BackupVaultResponse {
            encrypted_vault: row.encrypted_vault,
            salt: row.salt,
            mac: row.mac,
            updated_at: row.updated_at,
        })
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "No backup found").into_response(),
        Err(e) => {
            error!("Failed to fetch key backup: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

pub async fn delete_backup(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> impl IntoResponse {
    let username = &user.0.sub;

    let result = sqlx::query("DELETE FROM user_key_backups WHERE user_username = $1")
        .bind(username)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => (StatusCode::OK, "Backup deleted").into_response(),
        Err(e) => {
            error!("Failed to delete key backup: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}
