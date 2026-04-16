use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::{AppState, AuthenticatedUser};

const CURVE25519_KEY_LEN: usize = 32;
const CURVE25519_SIGNATURE_LEN: usize = 64;
const MAX_KEYS_PER_REQUEST: usize = 100;
const MAX_KEY_ID: i64 = 1_000_000;
const MAX_REGISTRATION_ID: i64 = 16380;
const MAX_DEVICES_PER_CLAIM: i64 = 100;

#[derive(Deserialize)]
pub struct ClaimKeysQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn validate_base64_key(key: &str, expected_len: usize) -> Result<Vec<u8>, &'static str> {
    let decoded = URL_SAFE_NO_PAD
        .decode(key)
        .map_err(|_| "Invalid Base64 encoding")?;
    if decoded.len() != expected_len {
        return Err("Invalid key length");
    }
    Ok(decoded)
}

#[derive(Deserialize, Debug)]
pub struct UploadKeysRequest {
    pub identity_key: String,
    pub registration_id: i64,
    pub signed_pre_key: SignedPreKeyRequest,
    pub one_time_keys: Vec<OneTimeKeyRequest>,
}

#[derive(Deserialize, Debug)]
pub struct SignedPreKeyRequest {
    pub key_id: i64,
    pub public_key: String,
    pub signature: String,
}

#[derive(Deserialize, Debug)]
pub struct OneTimeKeyRequest {
    pub key_id: i64,
    pub public_key: String,
}

pub async fn upload_keys(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<UploadKeysRequest>,
) -> impl IntoResponse {
    let username = user.0.sub;

    if payload.registration_id < 0 || payload.registration_id > MAX_REGISTRATION_ID {
        return (StatusCode::BAD_REQUEST, "Invalid registration_id").into_response();
    }

    if payload.signed_pre_key.key_id < 0 || payload.signed_pre_key.key_id > MAX_KEY_ID {
        return (StatusCode::BAD_REQUEST, "Invalid signed pre key ID").into_response();
    }

    if let Err(e) = validate_base64_key(&payload.identity_key, CURVE25519_KEY_LEN) {
        return (
            StatusCode::BAD_REQUEST,
            format!("Invalid identity_key: {}", e),
        )
            .into_response();
    }

    if let Err(e) = validate_base64_key(&payload.signed_pre_key.public_key, CURVE25519_KEY_LEN) {
        return (
            StatusCode::BAD_REQUEST,
            format!("Invalid signed pre key: {}", e),
        )
            .into_response();
    }

    if let Err(e) = validate_base64_key(&payload.signed_pre_key.signature, CURVE25519_SIGNATURE_LEN)
    {
        return (StatusCode::BAD_REQUEST, format!("Invalid signature: {}", e)).into_response();
    }

    if payload.one_time_keys.len() > MAX_KEYS_PER_REQUEST {
        return (StatusCode::BAD_REQUEST, "Too many one-time keys").into_response();
    }

    for otk in &payload.one_time_keys {
        if otk.key_id < 0 || otk.key_id > MAX_KEY_ID {
            return (StatusCode::BAD_REQUEST, "Invalid one-time key ID").into_response();
        }
        if let Err(e) = validate_base64_key(&otk.public_key, CURVE25519_KEY_LEN) {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid one-time key: {}", e),
            )
                .into_response();
        }
    }

    let session_id = match Uuid::parse_str(&user.0.session_id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid session ID").into_response(),
    };

    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            error!("Failed to start transaction: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let id_key_result = sqlx::query(
        "INSERT INTO device_identity_keys (user_username, device_id, registration_id, identity_key)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_username, device_id)
         DO UPDATE SET registration_id = EXCLUDED.registration_id, identity_key = EXCLUDED.identity_key",
    )
    .bind(&username)
    .bind(session_id)
    .bind(payload.registration_id)
    .bind(&payload.identity_key)
    .execute(&mut *tx)
    .await;

    if let Err(e) = id_key_result {
        error!("Failed to insert identity key: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
    }

    let spk_result = sqlx::query(
        "INSERT INTO device_signed_pre_keys (user_username, device_id, key_id, public_key, signature)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_username, device_id)
         DO UPDATE SET key_id = EXCLUDED.key_id, public_key = EXCLUDED.public_key, signature = EXCLUDED.signature",
    )
    .bind(&username)
    .bind(session_id)
    .bind(payload.signed_pre_key.key_id)
    .bind(&payload.signed_pre_key.public_key)
    .bind(&payload.signed_pre_key.signature)
    .execute(&mut *tx)
    .await;

    if let Err(e) = spk_result {
        error!("Failed to insert signed pre key: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
    }

    for otk in payload.one_time_keys {
        let otk_result = sqlx::query(
            "INSERT INTO device_one_time_keys (user_username, device_id, key_id, public_key, consumed)
             VALUES ($1, $2, $3, $4, FALSE)
             ON CONFLICT (user_username, device_id, key_id) DO NOTHING",
        )
        .bind(&username)
        .bind(session_id)
        .bind(otk.key_id)
        .bind(&otk.public_key)
        .execute(&mut *tx)
        .await;

        if let Err(e) = otk_result {
            error!("Failed to insert one time key: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        error!("Failed to commit transaction: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
    }

    info!(
        "Keys uploaded successfully for user {}, device {}",
        username, session_id
    );
    (StatusCode::OK, "Keys uploaded successfully").into_response()
}

#[derive(Serialize)]
pub struct ClaimKeysResponse {
    pub devices: Vec<DeviceKeys>,
}

#[derive(Serialize)]
pub struct DeviceKeys {
    pub device_id: String,
    pub identity_key: String,
    pub registration_id: i64,
    pub signed_pre_key: SignedPreKeyResponse,
    pub one_time_key: Option<OneTimeKeyResponse>,
    pub one_time_keys_remaining: i64,
}

#[derive(Serialize)]
pub struct SignedPreKeyResponse {
    pub key_id: i64,
    pub public_key: String,
    pub signature: String,
}

#[derive(Serialize)]
pub struct OneTimeKeyResponse {
    pub key_id: i64,
    pub public_key: String,
}

pub async fn claim_keys(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(target_username): Path<String>,
    Query(params): Query<ClaimKeysQuery>,
) -> impl IntoResponse {
    use sqlx::Row;

    let limit = params
        .limit
        .unwrap_or(MAX_DEVICES_PER_CLAIM)
        .min(MAX_DEVICES_PER_CLAIM);
    let offset = params.offset.unwrap_or(0).max(0);

    let identity_keys = sqlx::query(
        "SELECT d.device_id, d.identity_key, d.registration_id,
                spk.key_id AS spk_key_id, spk.public_key AS spk_public_key, spk.signature AS spk_signature
         FROM device_identity_keys d
         JOIN device_signed_pre_keys spk ON spk.device_id = d.device_id
         WHERE d.user_username = $1
         LIMIT $2 OFFSET $3",
    )
    .bind(&target_username)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await;

    let id_keys = match identity_keys {
        Ok(keys) => keys,
        Err(e) => {
            error!("Failed to fetch identity keys: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    if id_keys.is_empty() {
        return (StatusCode::NOT_FOUND, "No keys found for user").into_response();
    }

    let device_ids: Vec<Uuid> = id_keys.iter().map(|r| r.get("device_id")).collect();
    let device_ids_count = device_ids.len() as i64;

    let otk_rows = sqlx::query(
        "DELETE FROM device_one_time_keys
         WHERE id IN (
             SELECT id FROM device_one_time_keys
             WHERE device_id = ANY($1) AND consumed = FALSE
             ORDER BY device_id
             LIMIT $2
         )
         RETURNING device_id, key_id, public_key",
    )
    .bind(&device_ids)
    .bind(device_ids_count)
    .fetch_all(&state.db)
    .await;

    let consumed_otks: Vec<(Uuid, i64, String)> = match otk_rows {
        Ok(rows) => rows
            .iter()
            .map(|r| (r.get("device_id"), r.get("key_id"), r.get("public_key")))
            .collect(),
        Err(e) => {
            error!("Failed to consume OTKs: {}", e);
            Vec::new()
        }
    };

    let otk_count_rows = sqlx::query(
        "SELECT device_id, COUNT(*) as cnt FROM device_one_time_keys
         WHERE device_id = ANY($1) AND consumed = FALSE
         GROUP BY device_id",
    )
    .bind(&device_ids)
    .fetch_all(&state.db)
    .await;

    let otk_remaining: std::collections::HashMap<Uuid, i64> = match otk_count_rows {
        Ok(rows) => rows
            .iter()
            .map(|r| (r.get("device_id"), r.get::<i64, _>("cnt")))
            .collect(),
        Err(_) => std::collections::HashMap::new(),
    };

    let mut devices = Vec::new();

    for id_key in id_keys {
        let device_id: Uuid = id_key.get("device_id");
        let identity_key: String = id_key.get("identity_key");
        let registration_id: i64 = id_key.get("registration_id");
        let spk_key_id: i64 = id_key.get("spk_key_id");
        let spk_public_key: String = id_key.get("spk_public_key");
        let spk_signature: String = id_key.get("spk_signature");

        let consumed: Option<(i64, String)> = consumed_otks
            .iter()
            .find(|(did, _, _)| *did == device_id)
            .map(|(_, kid, pk)| (*kid, pk.clone()));

        let otk_resp = consumed.map(|(kid, pk)| OneTimeKeyResponse {
            key_id: kid,
            public_key: pk,
        });

        devices.push(DeviceKeys {
            device_id: device_id.to_string(),
            identity_key,
            registration_id,
            signed_pre_key: SignedPreKeyResponse {
                key_id: spk_key_id,
                public_key: spk_public_key,
                signature: spk_signature,
            },
            one_time_key: otk_resp,
            one_time_keys_remaining: otk_remaining.get(&device_id).copied().unwrap_or(0),
        });
    }

    (StatusCode::OK, Json(ClaimKeysResponse { devices })).into_response()
}

#[derive(Serialize)]
pub struct DeviceInfoResponse {
    pub device_id: Uuid,
    pub device_name: Option<String>,
    pub device_model: Option<String>,
    pub platform: Option<String>,
    pub registration_id: Option<i64>,
    pub one_time_keys_remaining: i64,
    pub created_at: DateTime<Utc>,
}

pub async fn list_devices(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> impl IntoResponse {
    use sqlx::Row;

    let username = &user.0.sub;

    let rows = sqlx::query(
        "SELECT s.id AS device_id, s.device_name, s.device_model, s.platform, s.created_at,
                dik.registration_id,
                (SELECT COUNT(*) FROM device_one_time_keys WHERE device_id = s.id AND consumed = FALSE) AS otk_remaining
         FROM sessions s
         LEFT JOIN device_identity_keys dik ON dik.device_id = s.id
         WHERE s.user_username = $1 AND s.is_valid = TRUE
         ORDER BY s.created_at DESC",
    )
    .bind(username)
    .fetch_all(&state.db)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to list devices: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let devices: Vec<DeviceInfoResponse> = rows
        .iter()
        .map(|r| {
            let created_at: DateTime<Utc> = r.get("created_at");
            DeviceInfoResponse {
                device_id: r.get("device_id"),
                device_name: r.get("device_name"),
                device_model: r.get("device_model"),
                platform: r.get("platform"),
                registration_id: r.get("registration_id"),
                one_time_keys_remaining: r.get("otk_remaining"),
                created_at,
            }
        })
        .collect();

    (StatusCode::OK, Json(devices)).into_response()
}

pub async fn delete_device(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(device_id): Path<Uuid>,
) -> impl IntoResponse {
    let username = &user.0.sub;

    let row = sqlx::query(
        "SELECT id FROM sessions WHERE id = $1 AND user_username = $2 AND is_valid = TRUE",
    )
    .bind(device_id)
    .bind(username)
    .fetch_optional(&state.db)
    .await;

    match row {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (StatusCode::FORBIDDEN, "Device not found or access denied").into_response();
        }
        Err(e) => {
            error!("Failed to verify device ownership: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    }

    let delete_result = sqlx::query("DELETE FROM sessions WHERE id = $1 AND user_username = $2")
        .bind(device_id)
        .bind(username)
        .execute(&state.db)
        .await;

    match delete_result {
        Ok(result) => {
            if result.rows_affected() == 0 {
                return (StatusCode::FORBIDDEN, "Device not found or access denied")
                    .into_response();
            }
            info!("Device {} deleted for user {}", device_id, username);
            (StatusCode::OK, "Device deleted successfully").into_response()
        }
        Err(e) => {
            error!("Failed to delete device: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}
