use api::worker::process_events_batch;
use shared::models::{DatabaseEvent, DeviceCiphertext};
use sqlx::PgPool;
use uuid::Uuid;

async fn create_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://invisible:password@127.0.0.1:5432/invisible_chat".to_string()
    });
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .unwrap()
}

async fn create_test_user(pool: &PgPool, username: &str) {
    sqlx::query("DELETE FROM messages WHERE sender_username = $1 OR recipient_username = $1")
        .bind(username)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM message_ciphertexts")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE username = $1")
        .bind(username)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, 'dummy')")
        .bind(username)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn process_events_batch_encrypted_messages() {
    let pool = create_test_pool().await;
    create_test_user(&pool, "alice_worker").await;
    create_test_user(&pool, "bob_worker").await;

    let alice_session_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, user_username) VALUES ($1, 'alice_worker')")
        .bind(alice_session_id)
        .execute(&pool)
        .await
        .unwrap();

    let msg_id = Uuid::new_v4().to_string();

    let events = vec![DatabaseEvent::NewEncryptedMessage {
        id: msg_id.clone(),
        sender: "alice_worker".to_string(),
        recipient: "bob_worker".to_string(),
        ciphertexts: vec![
            DeviceCiphertext {
                device_id: alice_session_id.to_string(),
                signal_type: 3,
                ciphertext: "test_ciphertext_1".to_string(),
            },
            DeviceCiphertext {
                device_id: Uuid::new_v4().to_string(),
                signal_type: 1,
                ciphertext: "test_ciphertext_2".to_string(),
            },
        ],
    }];

    process_events_batch(&pool, events).await.unwrap();

    let msg_row: (String, String, Option<String>) =
        sqlx::query_as("SELECT id, message_type, content FROM messages WHERE id = $1")
            .bind(&msg_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(msg_row.0, msg_id);
    assert_eq!(msg_row.1, "encrypted");
    assert!(msg_row.2.is_none());

    let ciphertexts: Vec<(String, i32, Uuid)> = sqlx::query_as(
        "SELECT ciphertext, signal_type, device_id FROM message_ciphertexts WHERE message_id = $1 ORDER BY signal_type",
    )
    .bind(&msg_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ciphertexts.len(), 2);
    assert_eq!(ciphertexts[0].1, 1);
    assert_eq!(ciphertexts[0].0, "test_ciphertext_2");
    assert_eq!(ciphertexts[1].1, 3);
    assert_eq!(ciphertexts[1].0, "test_ciphertext_1");
}

#[tokio::test]
async fn process_events_batch_mixed() {
    let pool = create_test_pool().await;
    create_test_user(&pool, "alice_mixed").await;
    create_test_user(&pool, "bob_mixed").await;

    let msg_id_1 = Uuid::new_v4().to_string();
    let msg_id_2 = Uuid::new_v4().to_string();
    let msg_id_3 = Uuid::new_v4().to_string();
    let bob_session_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, user_username) VALUES ($1, 'bob_mixed')")
        .bind(bob_session_id)
        .execute(&pool)
        .await
        .unwrap();

    let events = vec![
        DatabaseEvent::NewMessage {
            id: msg_id_1.clone(),
            sender: "alice_mixed".to_string(),
            recipient: "bob_mixed".to_string(),
            message_type: "text".to_string(),
            content: Some("Hello".to_string()),
            file_name: None,
            mime_type: None,
            file_url: None,
        },
        DatabaseEvent::NewEncryptedMessage {
            id: msg_id_2.clone(),
            sender: "alice_mixed".to_string(),
            recipient: "bob_mixed".to_string(),
            ciphertexts: vec![DeviceCiphertext {
                device_id: bob_session_id.to_string(),
                signal_type: 3,
                ciphertext: "enc_payload".to_string(),
            }],
        },
        DatabaseEvent::NewMessage {
            id: msg_id_3.clone(),
            sender: "bob_mixed".to_string(),
            recipient: "alice_mixed".to_string(),
            message_type: "text".to_string(),
            content: Some("Reply".to_string()),
            file_name: None,
            mime_type: None,
            file_url: None,
        },
    ];

    process_events_batch(&pool, events).await.unwrap();

    let msg1: (String, String) =
        sqlx::query_as("SELECT id, message_type FROM messages WHERE id = $1")
            .bind(&msg_id_1)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(msg1.0, msg_id_1);
    assert_eq!(msg1.1, "text");

    let msg2: (String, String) =
        sqlx::query_as("SELECT id, message_type FROM messages WHERE id = $1")
            .bind(&msg_id_2)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(msg2.0, msg_id_2);
    assert_eq!(msg2.1, "encrypted");

    let ct_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM message_ciphertexts WHERE message_id = $1")
            .bind(&msg_id_2)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ct_count.0, 1);

    let msg3: (String, String) =
        sqlx::query_as("SELECT id, message_type FROM messages WHERE id = $1")
            .bind(&msg_id_3)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(msg3.0, msg_id_3);
    assert_eq!(msg3.1, "text");
}

#[tokio::test]
async fn process_events_batch_read_receipt() {
    let pool = create_test_pool().await;
    create_test_user(&pool, "alice_receipt").await;
    create_test_user(&pool, "bob_receipt").await;

    let msg_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO messages (id, sender_username, recipient_username, message_type, content) VALUES ($1, 'alice_receipt', 'bob_receipt', 'text', 'hello')",
    )
    .bind(&msg_id)
    .execute(&pool)
    .await
    .unwrap();

    let events = vec![DatabaseEvent::ReadReceipt {
        message_id: msg_id.clone(),
        reader: "bob_receipt".to_string(),
    }];

    process_events_batch(&pool, events).await.unwrap();

    #[derive(Debug, sqlx::FromRow)]
    struct ReadState {
        unread_count: i32,
    }

    let state: Option<ReadState> = sqlx::query_as(
        "SELECT unread_count FROM dialog_read_states WHERE user_username = 'bob_receipt' AND peer_username = 'alice_receipt'"
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(state.is_some());
    assert_eq!(state.unwrap().unread_count, 0);
}
