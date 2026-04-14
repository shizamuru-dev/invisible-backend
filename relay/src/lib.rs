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
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info};

/// Shared application state
pub struct AppState {
    /// Maps user_id to an unbounded channel sender for that user
    pub clients: RwLock<HashMap<String, mpsc::UnboundedSender<String>>>,
    /// Queue for users who are currently offline
    pub offline_queue: RwLock<HashMap<String, Vec<OutgoingMessage>>>,
}

/// Creates the router for the relay server
pub fn app() -> Router {
    let app_state = Arc::new(AppState {
        clients: RwLock::new(HashMap::new()),
        offline_queue: RwLock::new(HashMap::new()),
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

    // Register the user and retrieve any offline messages
    let pending_messages = {
        let mut clients = state.clients.write().await;
        clients.insert(user_id.clone(), tx.clone());

        let mut queue = state.offline_queue.write().await;
        queue.remove(&user_id).unwrap_or_default()
    };

    // Send pending offline messages and send delivery receipts back to their senders
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
            let clients = state.clients.read().await;
            if let Some(sender_tx) = clients.get(&from) {
                let receipt = OutgoingMessage::DeliveryReceipt {
                    to: user_id.clone(),
                    message_id: id,
                };
                if let Ok(receipt_json) = serde_json::to_string(&receipt) {
                    let _ = sender_tx.send(receipt_json);
                }
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

    // Task 2: Receive messages from the client's WebSocket and forward them
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

                        if is_deliverable {
                            let mut is_delivered = false;

                            // Try to send to recipient if online
                            {
                                let clients = state_clone.clients.read().await;
                                if let Some(recipient_tx) = clients.get(&to) {
                                    #[allow(clippy::collapsible_if)]
                                    if let Ok(json_str) = serde_json::to_string(&outgoing) {
                                        is_delivered = recipient_tx.send(json_str).is_ok();
                                    }
                                }
                            }

                            if is_delivered {
                                // Recipient got it, send a delivery receipt to the sender
                                let receipt =
                                    OutgoingMessage::DeliveryReceipt { to, message_id: id };
                                let clients = state_clone.clients.read().await;
                                if let Some(sender_tx) = clients.get(&current_user_id) {
                                    #[allow(clippy::collapsible_if)]
                                    if let Ok(receipt_json) = serde_json::to_string(&receipt) {
                                        let _ = sender_tx.send(receipt_json);
                                    }
                                }
                            } else {
                                // Recipient is offline, put in queue
                                debug!("Recipient {} is offline. Queuing message {}", to, id);
                                let mut queue = state_clone.offline_queue.write().await;
                                queue.entry(to).or_default().push(outgoing);
                            }
                        } else {
                            // Non-deliverable messages like Typing indicators
                            let clients = state_clone.clients.read().await;
                            if let Some(recipient_tx) = clients.get(&to) {
                                #[allow(clippy::collapsible_if)]
                                if let Ok(json_str) = serde_json::to_string(&outgoing) {
                                    let _ = recipient_tx.send(json_str);
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

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    // Unregister the user on disconnect
    info!("User disconnected: {}", user_id);
    let mut clients = state.clients.write().await;
    clients.remove(&user_id);
}
