CREATE TABLE IF NOT EXISTS dialog_read_states (
    user_username VARCHAR(255) NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    peer_username VARCHAR(255) NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    last_read_message_id VARCHAR(255) NOT NULL,
    unread_count INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_username, peer_username)
);

CREATE INDEX idx_dialog_read_states_peer ON dialog_read_states(peer_username);
CREATE INDEX idx_dialog_read_states_message ON dialog_read_states(last_read_message_id);
