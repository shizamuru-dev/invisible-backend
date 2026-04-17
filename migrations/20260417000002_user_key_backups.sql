CREATE TABLE user_key_backups (
    user_username VARCHAR(255) PRIMARY KEY REFERENCES users(username) ON DELETE CASCADE,
    encrypted_vault TEXT NOT NULL,
    salt TEXT NOT NULL,
    mac TEXT NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP NOT NULL
);
