use api::AppState;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://invisible:password@127.0.0.1:5432/invisible_chat".to_string()
    });
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .unwrap();

    sqlx::migrate!("../migrations").run(&pool).await.unwrap();
    pool
}

pub async fn create_test_user_with_session() -> (PgPool, String, String, String) {
    let pool = create_test_pool().await;
    let secret = "test-secret-for-api".to_string();
    let username = format!(
        "user_{}",
        Uuid::new_v4()
            .to_string()
            .replace("-", "")
            .chars()
            .take(12)
            .collect::<String>()
    );
    let session_id = Uuid::new_v4().to_string();

    sqlx::query("DELETE FROM sessions WHERE user_username = $1")
        .bind(&username)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE username = $1")
        .bind(&username)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, 'dummy')")
        .bind(&username)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (id, user_username) VALUES ($1, $2)")
        .bind(Uuid::parse_str(&session_id).unwrap())
        .bind(&username)
        .execute(&pool)
        .await
        .unwrap();

    (pool, secret, session_id, username)
}

pub async fn create_test_user_with_session_named(
    username: &str,
) -> (PgPool, String, String, String) {
    let pool = create_test_pool().await;
    let secret = "test-secret-for-api".to_string();
    let session_id = Uuid::new_v4().to_string();

    sqlx::query("DELETE FROM sessions WHERE user_username = $1")
        .bind(username)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE username = $1")
        .bind(username)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, 'dummy')")
        .bind(username)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (id, user_username) VALUES ($1, $2)")
        .bind(Uuid::parse_str(&session_id).unwrap())
        .bind(username)
        .execute(&pool)
        .await
        .unwrap();

    (pool, secret, session_id, username.to_string())
}

pub fn create_test_state(pool: &PgPool, secret: &str) -> AppState {
    AppState {
        db: pool.clone(),
        jwt_secret: secret.to_string(),
        redis_client: redis::Client::open("redis://127.0.0.1:6379/").unwrap(),
        config: shared::config::AppConfig::default(),
    }
}

pub async fn upload_keys_for_user(
    pool: &PgPool,
    username: &str,
    session_id: &str,
    identity_key: &str,
    registration_id: i64,
) {
    let session_uuid = Uuid::parse_str(session_id).unwrap();
    sqlx::query(
        "INSERT INTO device_identity_keys (user_username, device_id, registration_id, identity_key)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_username, device_id) DO NOTHING",
    )
    .bind(username)
    .bind(session_uuid)
    .bind(registration_id)
    .bind(identity_key)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO device_signed_pre_keys (user_username, device_id, key_id, public_key, signature)
         VALUES ($1, $2, 1, 'pubkey', 'signature')
         ON CONFLICT (user_username, device_id) DO NOTHING",
    )
    .bind(username)
    .bind(session_uuid)
    .execute(pool)
    .await
    .unwrap();

    for i in 1..=3 {
        sqlx::query(
            "INSERT INTO device_one_time_keys (user_username, device_id, key_id, public_key, consumed)
             VALUES ($1, $2, $3, $4, FALSE)
             ON CONFLICT (user_username, device_id, key_id) DO NOTHING",
        )
        .bind(username)
        .bind(session_uuid)
        .bind(i)
        .bind(format!("otk_{}", i))
        .execute(pool)
        .await
        .unwrap();
    }
}
