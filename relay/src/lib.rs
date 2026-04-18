use axum::{
    Router,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use futures::{sink::SinkExt, stream::StreamExt};
use jsonwebtoken::{DecodingKey, Validation, decode};
use redis::AsyncCommands;
use shared::models::{Claims, IncomingMessage, OutgoingMessage, WsQuery};
use shared::repository::{OfflineMessageRepository, PresenceRepository, PubSubRepository};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Shared application state
pub struct AppState {
    /// Postgres connection pool
    pub pg_pool: PgPool,
    /// Repository for offline messages
    pub offline_repo: Arc<dyn OfflineMessageRepository>,
    /// Redis client for creating PubSub connections
    pub redis_client: redis::Client,
    /// Repository for publishing messages
    pub pubsub_repo: Arc<dyn PubSubRepository>,
    /// Repository for presence tracking
    pub presence_repo: Arc<dyn PresenceRepository>,
    /// JWT secret key
    pub jwt_secret: String,
}

/// Creates the router for the relay server
pub fn app(
    pg_pool: PgPool,
    offline_repo: Arc<dyn OfflineMessageRepository>,
    redis_client: redis::Client,
    pubsub_repo: Arc<dyn PubSubRepository>,
    presence_repo: Arc<dyn PresenceRepository>,
    jwt_secret: String,
) -> Router {
    let app_state = Arc::new(AppState {
        pg_pool,
        offline_repo,
        redis_client,
        pubsub_repo,
        presence_repo,
        jwt_secret,
    });

    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(app_state)
}

/// Handler for upgrading HTTP connections to WebSockets
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let token = query.token;

    let claims = match decode::<Claims>(
        &token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::default(),
    ) {
        Ok(c) => c.claims,
        Err(_) => {
            return (StatusCode::UNAUTHORIZED, "Invalid or expired token").into_response();
        }
    };

    let user_id = claims.sub;

    let session_id = match Uuid::parse_str(&claims.session_id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid session ID format").into_response(),
    };

    let session_key = format!("session:{}", session_id);
    let mut redis_conn = match state.redis_client.get_multiplexed_async_connection().await {
        Ok(conn) => conn,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Redis connection error").into_response();
        }
    };

    // 1. Check Redis cache first
    let cached_session: Option<String> = redis_conn.get(&session_key).await.unwrap_or(None);

    let is_valid = if cached_session.is_some() {
        // Update TTL
        let _: () = redis_conn
            .expire(&session_key, 30 * 24 * 60 * 60)
            .await
            .unwrap_or(());
        true
    } else {
        // 2. Cache miss, check PostgreSQL
        let session_valid: Option<bool> =
            match sqlx::query_scalar("SELECT is_valid FROM sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&state.pg_pool)
                .await
            {
                Ok(v) => v,
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
                }
            };

        if session_valid == Some(true) {
            // Populate cache
            let _: () = redis_conn
                .set_ex(&session_key, user_id.clone(), 30 * 24 * 60 * 60)
                .await
                .unwrap_or(());
            true
        } else {
            false
        }
    };

    if !is_valid {
        return (StatusCode::UNAUTHORIZED, "Session invalid or expired").into_response();
    }

    // Optionally update last accessed for the session in DB
    let pg_pool = state.pg_pool.clone();
    tokio::spawn(async move {
        let _ =
            sqlx::query("UPDATE sessions SET last_accessed_at = CURRENT_TIMESTAMP WHERE id = $1")
                .bind(session_id)
                .execute(&pg_pool)
                .await;
    });

    ws.on_upgrade(move |socket| handle_socket(socket, user_id, state))
}

/// Main logic for a single WebSocket connection
pub async fn handle_socket(socket: WebSocket, user_id: String, state: Arc<AppState>) {
    let conn_id = Uuid::new_v4().to_string();
    info!("User connected: {} (conn_id: {})", user_id, conn_id);

    // Add connection to presence repo
    match state.presence_repo.add_connection(&user_id, &conn_id).await {
        Ok(is_first) => {
            if is_first {
                let presence_msg = OutgoingMessage::PresenceUpdate {
                    user_id: user_id.clone(),
                    is_online: true,
                };
                if let Ok(json) = serde_json::to_string(&presence_msg) {
                    let _ = state
                        .pubsub_repo
                        .publish_message(&format!("presence:{}", user_id), &json)
                        .await;
                }
            }
        }
        Err(e) => error!("Failed to add connection for {}: {}", user_id, e),
    }

    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // 1. Fetch offline messages from Postgres
    let pending_messages = match state
        .offline_repo
        .fetch_and_delete_offline_messages(&user_id)
        .await
    {
        Ok(messages) => messages,
        Err(e) => {
            error!("Failed to fetch offline messages for {}: {}", user_id, e);
            Vec::new()
        }
    };

    // Send pending offline messages and send delivery receipts back to their senders via Redis
    for msg in pending_messages {
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = tx.send(json); // Send to current user
        }

        // If it was a text or file message, notify the original sender that it was delivered
        let receipt_info = match &msg {
            OutgoingMessage::Text { from, id, .. }
            | OutgoingMessage::Encrypted { from, id, .. }
            | OutgoingMessage::File { from, id, .. } => Some((from.clone(), id.clone())),
            _ => None,
        };

        if let Some((from, id)) = receipt_info {
            let receipt = OutgoingMessage::DeliveryReceipt {
                to: user_id.clone(),
                message_id: id,
            };
            if let Ok(receipt_json) = serde_json::to_string(&receipt) {
                let channel = format!("user:{}", from);
                let pubsub_repo = state.pubsub_repo.clone();
                tokio::spawn(async move {
                    if let Err(e) = pubsub_repo.publish_message(&channel, &receipt_json).await {
                        error!(
                            "Failed to publish delivery receipt for message from {}: {}",
                            from, e
                        );
                    }
                });
            }
        }
    }

    // Task 1: Receive messages from the channel and send them to the client's WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Task 2: Listen to Redis PubSub for this user and forward to the local channel
    let pubsub_client = state.redis_client.clone();
    let channel_name = format!("user:{}", user_id);
    let tx_pubsub = tx.clone();
    let mut pubsub_task = tokio::spawn(async move {
        if let Ok(mut pubsub) = pubsub_client.get_async_pubsub().await
            && pubsub.subscribe(&channel_name).await.is_ok()
        {
            let mut stream = pubsub.on_message();
            while let Some(msg) = stream.next().await {
                if let Ok(payload) = msg.get_payload::<String>() {
                    let _ = tx_pubsub.send(payload);
                }
            }
        }
    });

    // Task 3: Receive messages from the client's WebSocket and route them via Redis
    let state_clone = state.clone();
    let current_user_id = user_id.clone();
    let tx_recv = tx.clone();

    let mut redis_conn_for_events = match state_clone
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => Some(c),
        Err(e) => {
            error!(
                "Failed to get redis connection for queueing messages: {}",
                e
            );
            None
        }
    };

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                match serde_json::from_str::<IncomingMessage>(&text) {
                    Ok(parsed_msg) => {
                        let (requires_receipt, store_offline, to, id, outgoing) = match parsed_msg {
                            IncomingMessage::Encrypted {
                                to,
                                id,
                                ciphertexts,
                            } => (
                                true,
                                true,
                                to,
                                id.clone(),
                                OutgoingMessage::Encrypted {
                                    from: current_user_id.clone(),
                                    id,
                                    ciphertexts,
                                },
                            ),
                            IncomingMessage::Text { to, id, content } => (
                                true,
                                true,
                                to,
                                id.clone(),
                                OutgoingMessage::Text {
                                    from: current_user_id.clone(),
                                    id,
                                    content,
                                },
                            ),
                            IncomingMessage::File {
                                to,
                                id,
                                file_name,
                                mime_type,
                                file_url,
                            } => (
                                true,
                                true,
                                to,
                                id.clone(),
                                OutgoingMessage::File {
                                    from: current_user_id.clone(),
                                    id,
                                    file_name,
                                    mime_type,
                                    file_url,
                                },
                            ),
                            IncomingMessage::ReadReceipt { to, message_id } => (
                                false,
                                true,
                                to,
                                String::new(),
                                OutgoingMessage::ReadReceipt {
                                    from: current_user_id.clone(),
                                    message_id,
                                },
                            ),
                            IncomingMessage::Typing { to } => (
                                false,
                                false,
                                to,
                                String::new(),
                                OutgoingMessage::Typing {
                                    from: current_user_id.clone(),
                                },
                            ),
                            IncomingMessage::WatchPresence { user_ids } => {
                                for uid in user_ids {
                                    let is_online = state_clone
                                        .presence_repo
                                        .is_online(&uid)
                                        .await
                                        .unwrap_or(false);
                                    let presence = OutgoingMessage::PresenceUpdate {
                                        user_id: uid.clone(),
                                        is_online,
                                    };
                                    if let Ok(json) = serde_json::to_string(&presence) {
                                        let _ = tx_recv.send(json);
                                    }

                                    // Subscribe to presence changes for this user
                                    let pubsub_client = state_clone.redis_client.clone();
                                    let channel = format!("presence:{}", uid);
                                    let tx_clone = tx_recv.clone();
                                    tokio::spawn(async move {
                                        if let Ok(mut pubsub) =
                                            pubsub_client.get_async_pubsub().await
                                            && pubsub.subscribe(&channel).await.is_ok()
                                        {
                                            let mut stream = pubsub.on_message();
                                            while let Some(msg) = stream.next().await {
                                                if let Ok(payload) = msg.get_payload::<String>() {
                                                    let _ = tx_clone.send(payload);
                                                }
                                            }
                                        }
                                    });
                                }
                                continue;
                            }
                        };

                        // Save message to history asynchronously via Redis
                        if let Some(r) = redis_conn_for_events.as_mut() {
                            let db_event = match &outgoing {
                                OutgoingMessage::Encrypted {
                                    from,
                                    id,
                                    ciphertexts,
                                } => Some(shared::models::DatabaseEvent::NewEncryptedMessage {
                                    id: id.clone(),
                                    sender: from.clone(),
                                    recipient: to.clone(),
                                    ciphertexts: ciphertexts.clone(),
                                }),
                                OutgoingMessage::Text { from, id, content } => {
                                    Some(shared::models::DatabaseEvent::NewMessage {
                                        id: id.clone(),
                                        sender: from.clone(),
                                        recipient: to.clone(),
                                        message_type: "text".to_string(),
                                        content: Some(content.clone()),
                                        file_name: None,
                                        mime_type: None,
                                        file_url: None,
                                    })
                                }
                                OutgoingMessage::File {
                                    from,
                                    id,
                                    file_name,
                                    mime_type,
                                    file_url,
                                } => Some(shared::models::DatabaseEvent::NewMessage {
                                    id: id.clone(),
                                    sender: from.clone(),
                                    recipient: to.clone(),
                                    message_type: "file".to_string(),
                                    content: None,
                                    file_name: Some(file_name.clone()),
                                    mime_type: Some(mime_type.clone()),
                                    file_url: Some(file_url.clone()),
                                }),
                                OutgoingMessage::ReadReceipt { message_id, .. } => {
                                    Some(shared::models::DatabaseEvent::ReadReceipt {
                                        message_id: message_id.clone(),
                                        reader: current_user_id.clone(),
                                    })
                                }
                                _ => None,
                            };

                            if let Some(event) = db_event
                                && let Ok(json) = serde_json::to_string(&event)
                            {
                                let _: () = redis::cmd("XADD")
                                    .arg("message_events")
                                    .arg("*")
                                    .arg("event")
                                    .arg(json)
                                    .query_async(r)
                                    .await
                                    .unwrap_or_else(|e: redis::RedisError| {
                                        error!("Failed to xadd message_events to Redis: {}", e);
                                    });
                            }
                        }

                        if let Ok(json_str) = serde_json::to_string(&outgoing) {
                            let channel = format!("user:{}", to);

                            // Publish to Redis
                            let receivers = state_clone
                                .pubsub_repo
                                .publish_message(&channel, &json_str)
                                .await
                                .unwrap_or(0);

                            if receivers > 0 {
                                // Delivered successfully via Redis, send delivery receipt to sender if needed
                                if requires_receipt {
                                    let receipt = OutgoingMessage::DeliveryReceipt {
                                        to: to.clone(),
                                        message_id: id.clone(),
                                    };
                                    if let Ok(receipt_json) = serde_json::to_string(&receipt) {
                                        // Sender is current_user_id, send to their channel
                                        let sender_channel = format!("user:{}", current_user_id);
                                        if let Err(e) = state_clone
                                            .pubsub_repo
                                            .publish_message(&sender_channel, &receipt_json)
                                            .await
                                        {
                                            error!(
                                                "Failed to publish delivery receipt to sender {}: {}",
                                                current_user_id, e
                                            );
                                        }
                                    }
                                }
                            } else if store_offline {
                                // Nobody is subscribed, save to Postgres offline queue
                                debug!("Recipient {} is offline. Queuing message {}", to, id);
                                if let Ok(payload) = serde_json::to_value(&outgoing)
                                    && let Err(e) = state_clone
                                        .offline_repo
                                        .save_offline_message(&to, &payload)
                                        .await
                                {
                                    error!("Failed to save offline message to {}: {}", to, e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse message from {}: {}", current_user_id, e);
                    }
                }
            }
        }
    });

    // Wait for any task to finish, then abort the others
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
            pubsub_task.abort();
        },
        _ = &mut recv_task => {
            send_task.abort();
            pubsub_task.abort();
        },
        _ = &mut pubsub_task => {
            send_task.abort();
            recv_task.abort();
        }
    }

    match state
        .presence_repo
        .remove_connection(&user_id, &conn_id)
        .await
    {
        Ok(is_last) => {
            if is_last {
                let presence_msg = OutgoingMessage::PresenceUpdate {
                    user_id: user_id.clone(),
                    is_online: false,
                };
                if let Ok(json) = serde_json::to_string(&presence_msg) {
                    let _ = state
                        .pubsub_repo
                        .publish_message(&format!("presence:{}", user_id), &json)
                        .await;
                }
            }
        }
        Err(e) => error!("Failed to remove connection for {}: {}", user_id, e),
    }

    info!("User disconnected: {}", user_id);
}
