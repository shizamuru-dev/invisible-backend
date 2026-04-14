use std::{collections::HashMap, sync::Arc};

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
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info};

/// Query parameters for the WebSocket connection
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub user_id: String,
}

/// Message format expected from the client
#[derive(Debug, Deserialize, Serialize)]
pub struct IncomingMessage {
    pub to: String,
    pub content: String,
}

/// Message format sent to the recipient
#[derive(Debug, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub from: String,
    pub content: String,
}

/// Shared application state
pub struct AppState {
    /// Maps user_id to an unbounded channel sender for that user
    pub clients: RwLock<HashMap<String, mpsc::UnboundedSender<String>>>,
}

/// Creates the router for the relay server
pub fn app() -> Router {
    let app_state = Arc::new(AppState {
        clients: RwLock::new(HashMap::new()),
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

    // Create an unbounded channel for this user's incoming messages from the server
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register the user
    {
        let mut clients = state.clients.write().await;
        clients.insert(user_id.clone(), tx);
    }

    // Task 1: Receive messages from the channel and send them to the client's WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                // Client probably disconnected
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
                // Parse the incoming JSON message
                match serde_json::from_str::<IncomingMessage>(&text) {
                    Ok(parsed_msg) => {
                        let clients = state_clone.clients.read().await;
                        if let Some(recipient_tx) = clients.get(&parsed_msg.to) {
                            let outgoing = OutgoingMessage {
                                from: current_user_id.clone(),
                                content: parsed_msg.content,
                            };
                            if let Ok(json_str) = serde_json::to_string(&outgoing) {
                                // Ignore send errors (e.g., recipient channel closed)
                                let _ = recipient_tx.send(json_str);
                            }
                        } else {
                            debug!("Recipient {} not found", parsed_msg.to);
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
