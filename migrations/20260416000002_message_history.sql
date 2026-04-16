CREATE TABLE IF NOT EXISTS messages (
    id VARCHAR(255) PRIMARY KEY,
    sender_username VARCHAR(255) REFERENCES users(username) ON DELETE CASCADE,
    recipient_username VARCHAR(255) REFERENCES users(username) ON DELETE CASCADE,
    message_type VARCHAR(50) NOT NULL,
    content TEXT,
    file_name VARCHAR(255),
    mime_type VARCHAR(100),
    file_url VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    read_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_messages_sender_recipient ON messages(sender_username, recipient_username);
CREATE INDEX idx_messages_recipient_sender ON messages(recipient_username, sender_username);
CREATE INDEX idx_messages_created_at ON messages(created_at DESC);
