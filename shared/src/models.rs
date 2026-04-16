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
    pub session_id: String,
    pub exp: usize,
}

/// Device info provided on login
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceInfo {
    pub device_name: String,
    pub device_model: String,
    pub platform: String,
    pub hwid: String,
}

/// Per-device ciphertext payload for E2EE message delivery.
/// Used by both Signal Protocol (1-1 chats) and Olm/Megolm (groups, future).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceCiphertext {
    /// Target device UUID (matches `sessions.id`)
    pub device_id: String,
    /// Signal Protocol message type: 1 = Normal (Whisper), 3 = PreKey (initial handshake).
    /// For future Megolm: 0 = Megolm ciphertext.
    pub signal_type: i32,
    /// Base64-encoded ciphertext body
    pub ciphertext: String,
}

/// Message format expected from the client
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum IncomingMessage {
    Encrypted {
        to: String,
        id: String,
        ciphertexts: Vec<DeviceCiphertext>,
    },
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
    ReadReceipt {
        to: String,
        message_id: String,
    },
}

/// Message format sent to the recipient (or sender for receipts)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum OutgoingMessage {
    Encrypted {
        from: String,
        id: String,
        ciphertexts: Vec<DeviceCiphertext>,
    },
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
    ReadReceipt {
        from: String,
        message_id: String,
    },
    PresenceUpdate {
        user_id: String,
        is_online: bool,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DatabaseEvent {
    NewEncryptedMessage {
        id: String,
        sender: String,
        recipient: String,
        ciphertexts: Vec<DeviceCiphertext>,
    },
    NewMessage {
        id: String,
        sender: String,
        recipient: String,
        message_type: String,
        content: Option<String>,
        file_name: Option<String>,
        mime_type: Option<String>,
        file_url: Option<String>,
    },
    ReadReceipt {
        message_id: String,
    },
}
