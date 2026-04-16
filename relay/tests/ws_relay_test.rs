use futures::{SinkExt, StreamExt};
use jsonwebtoken::{EncodingKey, Header, encode};
use relay::app;
use shared::models::{Claims, DeviceCiphertext, IncomingMessage, OutgoingMessage};
use shared::repository::{
    PgOfflineMessageRepository, RedisPresenceRepository, RedisPubSubRepository,
};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as TgMessage};
use uuid::Uuid;

async fn generate_token_and_session(username: &str, pg_pool: &PgPool) -> String {
    let session_id = Uuid::new_v4();

    // Migrations are run by start_test_server(), tables already exist

    // Insert dummy user if not exists
    sqlx::query(
        "INSERT INTO users (username, password_hash) VALUES ($1, 'test') ON CONFLICT DO NOTHING",
    )
    .bind(username)
    .execute(pg_pool)
    .await
    .unwrap();

    // Insert session
    sqlx::query("INSERT INTO sessions (id, user_username) VALUES ($1, $2)")
        .bind(session_id)
        .bind(username)
        .execute(pg_pool)
        .await
        .unwrap();

    let claims = Claims {
        sub: username.to_string(),
        session_id: session_id.to_string(),
        exp: (sqlx::types::chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

/// Helper function to start the server in the background and return its address
async fn start_test_server() -> (SocketAddr, PgPool) {
    // For tests, use environment variables to connect to actual DBs
    unsafe {
        std::env::set_var(
            "DATABASE_URL",
            "postgres://invisible:password@127.0.0.1:5432/invisible_chat",
        );
        std::env::set_var("REDIS_URL", "redis://127.0.0.1:6379/");
    }

    let config = shared::config::AppConfig::load().unwrap_or_default();
    let pg_pool = shared::db::init_postgres(&config).await.unwrap();
    let (redis_client, redis_manager) = shared::db::init_redis(&config).await.unwrap();

    sqlx::migrate!("../migrations").run(&pg_pool).await.unwrap();

    let offline_repo = Arc::new(PgOfflineMessageRepository::new(pg_pool.clone()));
    let pubsub_repo = Arc::new(RedisPubSubRepository::new(redis_manager.clone()));
    let presence_repo = Arc::new(RedisPresenceRepository::new(redis_manager));
    let jwt_secret = "test-secret".to_string();

    let app = app(
        pg_pool.clone(),
        offline_repo,
        redis_client,
        pubsub_repo,
        presence_repo,
        jwt_secret,
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (addr, pg_pool)
}

#[tokio::test]
async fn given_two_users_when_one_sends_message_then_other_receives_it() {
    let (addr, pg_pool) = start_test_server().await;

    let alice_token = generate_token_and_session("alice", &pg_pool).await;
    let alice_url = format!("ws://{}/ws?token={}", addr, alice_token);
    let (mut alice_ws, _) = connect_async(&alice_url).await.unwrap();

    let bob_token = generate_token_and_session("bob", &pg_pool).await;
    let bob_url = format!("ws://{}/ws?token={}", addr, bob_token);
    let (mut bob_ws, _) = connect_async(&bob_url).await.unwrap();

    // Small delay to ensure Redis PubSub is active before sending messages
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 1. Text Message
    let text_incoming = IncomingMessage::Text {
        to: "bob".to_string(),
        id: "msg-123".to_string(),
        content: "Hello, Bob!".to_string(),
    };
    alice_ws
        .send(TgMessage::Text(
            serde_json::to_string(&text_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    // Alice should immediately get a delivery receipt
    let msg = alice_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "bob");
                assert_eq!(message_id, "msg-123");
            }
            _ => panic!("Expected DeliveryReceipt message for Alice"),
        }
    } else {
        panic!("Expected text message");
    }

    // Bob should get the text message
    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Text { from, id, content } => {
                assert_eq!(from, "alice");
                assert_eq!(id, "msg-123");
                assert_eq!(content, "Hello, Bob!");
            }
            _ => panic!("Expected Text message"),
        }
    } else {
        panic!("Expected text message");
    }

    // 2. Typing Indicator
    let typing_incoming = IncomingMessage::Typing {
        to: "bob".to_string(),
    };
    alice_ws
        .send(TgMessage::Text(
            serde_json::to_string(&typing_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Typing { from } => {
                assert_eq!(from, "alice");
            }
            _ => panic!("Expected Typing message"),
        }
    } else {
        panic!("Expected text message");
    }

    // 3. Read Receipt
    let read_incoming = IncomingMessage::ReadReceipt {
        to: "alice".to_string(),
        message_id: "msg-123".to_string(),
    };
    bob_ws
        .send(TgMessage::Text(
            serde_json::to_string(&read_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    let msg = alice_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::ReadReceipt { from, message_id } => {
                assert_eq!(from, "bob");
                assert_eq!(message_id, "msg-123");
            }
            _ => panic!("Expected ReadReceipt message"),
        }
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn given_offline_user_when_message_sent_then_received_on_connect() {
    let (addr, pg_pool) = start_test_server().await;

    // Connect user "charlie"
    let charlie_token = generate_token_and_session("charlie", &pg_pool).await;
    let charlie_url = format!("ws://{}/ws?token={}", addr, charlie_token);
    let (mut charlie_ws, _) = connect_async(&charlie_url).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Charlie sends a message to "dave" (who is offline)
    let incoming = IncomingMessage::Text {
        to: "dave".to_string(),
        id: "msg-offline-1".to_string(),
        content: "Hey Dave, this is waiting for you!".to_string(),
    };
    charlie_ws
        .send(TgMessage::Text(
            serde_json::to_string(&incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    // Give the server a tiny bit of time to queue it
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Now Dave connects
    let dave_token = generate_token_and_session("dave", &pg_pool).await;
    let dave_url = format!("ws://{}/ws?token={}", addr, dave_token);
    let (mut dave_ws, _) = connect_async(&dave_url).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Dave should immediately receive the queued message
    let msg = dave_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Text { from, id, content } => {
                assert_eq!(from, "charlie");
                assert_eq!(id, "msg-offline-1");
                assert_eq!(content, "Hey Dave, this is waiting for you!");
            }
            _ => panic!("Expected queued Text message"),
        }
    } else {
        panic!("Expected text message");
    }

    // Charlie should now receive the delivery receipt since Dave got it
    let msg = charlie_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "dave");
                assert_eq!(message_id, "msg-offline-1");
            }
            _ => panic!("Expected DeliveryReceipt message for Charlie"),
        }
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn given_two_users_when_file_sent_then_received() {
    let (addr, pg_pool) = start_test_server().await;

    let alice_token = generate_token_and_session("alice", &pg_pool).await;
    let alice_url = format!("ws://{}/ws?token={}", addr, alice_token);
    let (mut alice_ws, _) = connect_async(&alice_url).await.unwrap();

    let bob_token = generate_token_and_session("bob", &pg_pool).await;
    let bob_url = format!("ws://{}/ws?token={}", addr, bob_token);
    let (mut bob_ws, _) = connect_async(&bob_url).await.unwrap();

    // Small delay to ensure Redis PubSub is active before sending messages
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Alice sends a File message to Bob
    let file_incoming = IncomingMessage::File {
        to: "bob".to_string(),
        id: "msg-file-1".to_string(),
        file_name: "hello.txt".to_string(),
        mime_type: "text/plain".to_string(),
        file_url: "/download/12345".to_string(),
    };
    alice_ws
        .send(TgMessage::Text(
            serde_json::to_string(&file_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    // Alice should get a delivery receipt
    let msg = alice_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "bob");
                assert_eq!(message_id, "msg-file-1");
            }
            _ => panic!("Expected DeliveryReceipt message for Alice's file"),
        }
    } else {
        panic!("Expected text message");
    }

    // Bob should get the file message
    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::File {
                from,
                id,
                file_name,
                mime_type,
                file_url,
            } => {
                assert_eq!(from, "alice");
                assert_eq!(id, "msg-file-1");
                assert_eq!(file_name, "hello.txt");
                assert_eq!(mime_type, "text/plain");
                assert_eq!(file_url, "/download/12345");
            }
            _ => panic!("Expected File message"),
        }
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn given_two_users_when_encrypted_message_sent_then_other_receives_it() {
    let (addr, pg_pool) = start_test_server().await;

    let alice_token = generate_token_and_session("alice_e2ee", &pg_pool).await;
    let alice_url = format!("ws://{}/ws?token={}", addr, alice_token);
    let (mut alice_ws, _) = connect_async(&alice_url).await.unwrap();

    let bob_token = generate_token_and_session("bob_e2ee", &pg_pool).await;
    let bob_url = format!("ws://{}/ws?token={}", addr, bob_token);
    let (mut bob_ws, _) = connect_async(&bob_url).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let bob_session_id: String = sqlx::query_scalar(
        "SELECT id::text FROM sessions WHERE user_username = 'bob_e2ee' LIMIT 1",
    )
    .fetch_one(&pg_pool)
    .await
    .unwrap();

    let encrypted_incoming = IncomingMessage::Encrypted {
        to: "bob_e2ee".to_string(),
        id: "msg-e2ee-1".to_string(),
        ciphertexts: vec![DeviceCiphertext {
            device_id: bob_session_id.clone(),
            signal_type: 3,
            ciphertext: "dmFsaWQgY2lwaGVydGV4dA==".to_string(),
        }],
    };
    alice_ws
        .send(TgMessage::Text(
            serde_json::to_string(&encrypted_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    // Alice should get a delivery receipt
    let msg = alice_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "bob_e2ee");
                assert_eq!(message_id, "msg-e2ee-1");
            }
            _ => panic!("Expected DeliveryReceipt for encrypted message"),
        }
    } else {
        panic!("Expected text message");
    }

    // Bob should receive the encrypted message with his device_id
    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Encrypted {
                from,
                id,
                ciphertexts,
            } => {
                assert_eq!(from, "alice_e2ee");
                assert_eq!(id, "msg-e2ee-1");
                assert_eq!(ciphertexts.len(), 1);
                assert_eq!(ciphertexts[0].device_id, bob_session_id);
                assert_eq!(ciphertexts[0].signal_type, 3);
                assert_eq!(ciphertexts[0].ciphertext, "dmFsaWQgY2lwaGVydGV4dA==");
            }
            _ => panic!("Expected Encrypted message"),
        }
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn given_offline_user_when_encrypted_message_sent_then_received_on_connect() {
    let (addr, pg_pool) = start_test_server().await;

    // Dave connects first to create a session, then disconnects
    let dave_token = generate_token_and_session("dave_e2ee", &pg_pool).await;
    let dave_url = format!("ws://{}/ws?token={}", addr, dave_token);
    let (dave_ws, _) = connect_async(&dave_url).await.unwrap();
    drop(dave_ws); // Disconnect Dave

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Get Dave's session ID after he's connected and disconnected
    let dave_session_id: String = sqlx::query_scalar(
        "SELECT id::text FROM sessions WHERE user_username = 'dave_e2ee' LIMIT 1",
    )
    .fetch_one(&pg_pool)
    .await
    .unwrap();

    // Charlie connects and sends encrypted message to Dave
    let charlie_token = generate_token_and_session("charlie_e2ee", &pg_pool).await;
    let charlie_url = format!("ws://{}/ws?token={}", addr, charlie_token);
    let (mut charlie_ws, _) = connect_async(&charlie_url).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let encrypted_incoming = IncomingMessage::Encrypted {
        to: "dave_e2ee".to_string(),
        id: "msg-e2ee-offline".to_string(),
        ciphertexts: vec![DeviceCiphertext {
            device_id: dave_session_id,
            signal_type: 3,
            ciphertext: "b2ZmbGluZSBtZXNzYWdl".to_string(),
        }],
    };
    charlie_ws
        .send(TgMessage::Text(
            serde_json::to_string(&encrypted_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Dave reconnects
    let dave_url = format!("ws://{}/ws?token={}", addr, dave_token);
    let (mut dave_ws, _) = connect_async(&dave_url).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Dave should receive the queued encrypted message
    let msg = dave_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Encrypted {
                from,
                id,
                ciphertexts,
            } => {
                assert_eq!(from, "charlie_e2ee");
                assert_eq!(id, "msg-e2ee-offline");
                assert_eq!(ciphertexts.len(), 1);
                assert_eq!(ciphertexts[0].ciphertext, "b2ZmbGluZSBtZXNzYWdl");
            }
            _ => panic!("Expected queued Encrypted message"),
        }
    } else {
        panic!("Expected text message");
    }

    // Charlie should get a delivery receipt
    let msg = charlie_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "dave_e2ee");
                assert_eq!(message_id, "msg-e2ee-offline");
            }
            _ => panic!("Expected DeliveryReceipt for Charlie"),
        }
    } else {
        panic!("Expected text message");
    }
}
