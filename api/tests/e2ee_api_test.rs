mod fixtures;

use api::create_router;
use fixtures::{
    create_test_state, create_test_user_with_session, create_test_user_with_session_named,
    upload_keys_for_user,
};
use jsonwebtoken::{EncodingKey, Header, encode};

fn make_auth_header(secret: &str, username: &str, session_id: &str) -> String {
    let claims = serde_json::json!({
        "sub": username,
        "session_id": session_id,
        "exp": (chrono::Utc::now().timestamp() + 3600) as usize
    });
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

async fn spawn_test_server(state: api::AppState) -> String {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

#[tokio::test]
async fn upload_keys_success() {
    let (pool, secret, session_id, username) = create_test_user_with_session().await;
    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/keys/upload", base_url))
        .header(
            "Authorization",
            make_auth_header(&secret, &username, &session_id),
        )
        .json(&serde_json::json!({
            "identity_key": "aWNlbnRp",
            "registration_id": 42,
            "signed_pre_key": {
                "key_id": 1,
                "public_key": "c3BrcHVi",
                "signature": "c2lnbmF0dXJl"
            },
            "one_time_keys": [
                { "key_id": 1, "public_key": "b3RrMTAx" },
                { "key_id": 2, "public_key": "b3RrMTAy" }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_identity_keys WHERE user_username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);

    let otk_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_one_time_keys WHERE user_username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(otk_count.0, 2);
}

#[tokio::test]
async fn upload_keys_identity_key_upsert() {
    let (pool, secret, session_id, username) = create_test_user_with_session().await;
    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let upload = |ik: &str| {
        let token = make_auth_header(&secret, &username, &session_id);
        client
            .post(format!("{}/keys/upload", base_url))
            .header("Authorization", token)
            .json(&serde_json::json!({
                "identity_key": ik,
                "registration_id": 42,
                "signed_pre_key": {
                    "key_id": 1,
                    "public_key": "c3BrcHVi",
                    "signature": "c2lnbmF0dXJl"
                },
                "one_time_keys": []
            }))
            .send()
    };

    upload("first_key").await.unwrap();
    upload("second_key").await.unwrap();

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_identity_keys WHERE user_username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);

    let current_key: (String,) =
        sqlx::query_as("SELECT identity_key FROM device_identity_keys WHERE user_username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(current_key.0, "second_key");
}

#[tokio::test]
async fn upload_keys_otk_duplicates_ignored() {
    let (pool, secret, session_id, username) = create_test_user_with_session().await;
    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let upload = || {
        let token = make_auth_header(&secret, &username, &session_id);
        client
            .post(format!("{}/keys/upload", base_url))
            .header("Authorization", token)
            .json(&serde_json::json!({
                "identity_key": "aWNlbnRp",
                "registration_id": 42,
                "signed_pre_key": {
                    "key_id": 1,
                    "public_key": "c3BrcHVi",
                    "signature": "c2lnbmF0dXJl"
                },
                "one_time_keys": [
                    { "key_id": 1, "public_key": "b3RrMTAx" },
                    { "key_id": 2, "public_key": "b3RrMTAy" },
                    { "key_id": 3, "public_key": "b3RrMTAz" }
                ]
            }))
            .send()
    };

    upload().await.unwrap();
    upload().await.unwrap();

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_one_time_keys WHERE user_username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 3);
}

#[tokio::test]
async fn claim_keys_success() {
    let (pool, secret, alice_session, alice) = create_test_user_with_session().await;
    let (_, _, bob_session, bob) = create_test_user_with_session_named("bob_claim").await;

    upload_keys_for_user(&pool, &alice, &alice_session, "alice_identity", 1).await;

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/keys/claim/{}", base_url, alice))
        .header(
            "Authorization",
            make_auth_header(&secret, &bob, &bob_session),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let parsed: serde_json::Value = res.json().await.unwrap();
    let devices = parsed.get("devices").unwrap().as_array().unwrap();
    assert!(!devices.is_empty());
    assert_eq!(
        devices[0].get("identity_key").unwrap().as_str().unwrap(),
        "alice_identity"
    );
}

#[tokio::test]
async fn claim_keys_otk_consumed() {
    let (pool, secret, alice_session, alice) = create_test_user_with_session().await;
    let (_, _, bob_session, bob) = create_test_user_with_session_named("bob_otk").await;

    upload_keys_for_user(&pool, &alice, &alice_session, "alice_otk", 1).await;

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let claim = || {
        let token = make_auth_header(&secret, &bob, &bob_session);
        client
            .get(format!("{}/keys/claim/{}", base_url, alice))
            .header("Authorization", token)
            .send()
    };

    let res1 = claim().await.unwrap();
    assert_eq!(res1.status(), 200);
    let parsed1: serde_json::Value = res1.json().await.unwrap();
    assert!(
        !parsed1["devices"][0]["one_time_key"].is_null(),
        "First claim should return an OTK"
    );

    let res2 = claim().await.unwrap();
    assert_eq!(res2.status(), 200);
    let parsed2: serde_json::Value = res2.json().await.unwrap();
    assert!(
        parsed2["devices"][0]["one_time_key"].is_null(),
        "Second claim should have null OTK"
    );
    assert_eq!(
        parsed2["devices"][0]["one_time_keys_remaining"]
            .as_i64()
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn claim_keys_no_keys_for_user() {
    let (pool, secret, _, bob) = create_test_user_with_session().await;

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/keys/claim/nonexistent_user_xyz", base_url))
        .header(
            "Authorization",
            make_auth_header(&secret, &bob, "no-session"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn claim_keys_unauthorized() {
    let (pool, _, _, _) = create_test_user_with_session().await;

    let state = create_test_state(&pool, "wrong-secret");
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/keys/claim/someuser", base_url))
        .header("Authorization", "Bearer invalid.token.here")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn list_devices_success() {
    let (pool, secret, session_id, username) = create_test_user_with_session().await;
    upload_keys_for_user(&pool, &username, &session_id, "test_identity", 42).await;

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/keys/devices", base_url))
        .header(
            "Authorization",
            make_auth_header(&secret, &username, &session_id),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let devices: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(!devices.is_empty());
    assert_eq!(
        devices[0].get("device_id").unwrap().as_str().unwrap(),
        session_id
    );
}

#[tokio::test]
async fn list_devices_shows_all_user_sessions() {
    let (pool, secret, session1, username) = create_test_user_with_session().await;
    let session2 = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (id, user_username) VALUES ($1, $2)")
        .bind(uuid::Uuid::parse_str(&session2).unwrap())
        .bind(&username)
        .execute(&pool)
        .await
        .unwrap();

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/keys/devices", base_url))
        .header(
            "Authorization",
            make_auth_header(&secret, &username, &session1),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let devices: Vec<serde_json::Value> = res.json().await.unwrap();
    assert_eq!(devices.len(), 2);
}

#[tokio::test]
async fn delete_device_success() {
    let (pool, secret, session_id, username) = create_test_user_with_session().await;
    let device_uuid = uuid::Uuid::parse_str(&session_id).unwrap();

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{}/keys/devices/{}", base_url, session_id))
        .header(
            "Authorization",
            make_auth_header(&secret, &username, &session_id),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let exists: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = $1)")
        .bind(device_uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!exists.0);
}

#[tokio::test]
async fn delete_device_not_owner() {
    let (pool, secret, alice_session, _alice) = create_test_user_with_session().await;
    let (_, _, bob_session, bob) = create_test_user_with_session_named("bob_delete").await;

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{}/keys/devices/{}", base_url, alice_session))
        .header(
            "Authorization",
            make_auth_header(&secret, &bob, &bob_session),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn delete_device_cascades_keys() {
    let (pool, secret, session_id, username) = create_test_user_with_session().await;
    let device_uuid = uuid::Uuid::parse_str(&session_id).unwrap();
    upload_keys_for_user(&pool, &username, &session_id, "cascade_identity", 1).await;

    let state = create_test_state(&pool, &secret);
    let base_url = spawn_test_server(state).await;

    let client = reqwest::Client::new();
    client
        .delete(format!("{}/keys/devices/{}", base_url, session_id))
        .header(
            "Authorization",
            make_auth_header(&secret, &username, &session_id),
        )
        .send()
        .await
        .unwrap();

    let id_keys: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_identity_keys WHERE device_id = $1")
            .bind(device_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(id_keys.0, 0);

    let spk_keys: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_signed_pre_keys WHERE device_id = $1")
            .bind(device_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(spk_keys.0, 0);

    let otk_keys: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_one_time_keys WHERE device_id = $1")
            .bind(device_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(otk_keys.0, 0);
}
