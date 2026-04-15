use serde::{Deserialize, Serialize};

/// Query parameters for the WebSocket connection
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: String,
}

/// JWT Claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

/// Message format expected from the client
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum IncomingMessage {
    Text {
        to: String,
        id: String,
        content: String,
    },
    File {
        to: String,
        id: String,
        file_name: String,
        mime_type: String,
        file_url: String, // Изменено с file_data на URL/ID сохранённого файла
    },
    Typing {
        to: String,
    },
    WatchPresence {
        user_ids: Vec<String>,
    },
}

/// Message format sent to the recipient (or sender for receipts)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum OutgoingMessage {
    Text {
        from: String,
        id: String,
        content: String,
    },
    File {
        from: String,
        id: String,
        file_name: String,
        mime_type: String,
        file_url: String, // Изменено
    },
    Typing {
        from: String,
    },
    DeliveryReceipt {
        to: String,
        message_id: String,
    },
    PresenceUpdate {
        user_id: String,
        is_online: bool,
    },
}
