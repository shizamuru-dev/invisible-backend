use axum::{
    Router,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures::{sink::SinkExt, stream::StreamExt};
use shared::models::{IncomingMessage, OutgoingMessage, WsQuery};
use shared::repository::{OfflineMessageRepository, PubSubRepository};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Shared application state
pub struct AppState {
    /// Repository for offline messages
    pub offline_repo: Arc<dyn OfflineMessageRepository>,
    /// Redis client for creating PubSub connections
    pub redis_client: redis::Client,
    /// Repository for publishing messages
    pub pubsub_repo: Arc<dyn PubSubRepository>,
}

/// Creates the router for the relay server
pub fn app(
    offline_repo: Arc<dyn OfflineMessageRepository>,
    redis_client: redis::Client,
    pubsub_repo: Arc<dyn PubSubRepository>,
) -> Router {
    let app_state = Arc::new(AppState {
        offline_repo,
        redis_client,
        pubsub_repo,
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
    ws.on_upgrade(|socket| handle_socket(socket, query.user_id, state))
}

/// Main logic for a single WebSocket connection
pub async fn handle_socket(socket: WebSocket, user_id: String, state: Arc<AppState>) {
    info!("User connected: {}", user_id);

    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // 1. Fetch offline messages from Postgres
    let pending_messages = match state.offline_repo.fetch_and_delete_offline_messages(&user_id).await {
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
            OutgoingMessage::Text { from, id, .. } | OutgoingMessage::File { from, id, .. } => {
                Some((from.clone(), id.clone()))
            }
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
                        error!("Failed to publish delivery receipt for message from {}: {}", from, e);
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
            && pubsub.subscribe(&channel_name).await.is_ok() {
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

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                match serde_json::from_str::<IncomingMessage>(&text) {
                    Ok(parsed_msg) => {
                        let (is_deliverable, to, id, outgoing) = match parsed_msg {
                            IncomingMessage::Text { to, id, content } => (
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
                            IncomingMessage::Typing { to } => (
                                false,
                                to,
                                String::new(),
                                OutgoingMessage::Typing {
                                    from: current_user_id.clone(),
                                },
                            ),
                        };

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
                                if is_deliverable {
                                    let receipt = OutgoingMessage::DeliveryReceipt { to: to.clone(), message_id: id };
                                    if let Ok(receipt_json) = serde_json::to_string(&receipt) {
                                        // Sender is current_user_id, send to their channel
                                        let sender_channel = format!("user:{}", current_user_id);
                                        if let Err(e) = state_clone.pubsub_repo.publish_message(&sender_channel, &receipt_json).await {
                                            error!("Failed to publish delivery receipt to sender {}: {}", current_user_id, e);
                                        }
                                    }
                                }
                            } else if is_deliverable {
                                // Nobody is subscribed, save to Postgres offline queue
                                debug!("Recipient {} is offline. Queuing message {}", to, id);
                                if let Ok(payload) = serde_json::to_value(&outgoing)
                                    && let Err(e) = state_clone.offline_repo.save_offline_message(&to, &payload).await {
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

    info!("User disconnected: {}", user_id);
}
