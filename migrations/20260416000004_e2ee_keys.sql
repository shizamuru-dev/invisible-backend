CREATE TABLE IF NOT EXISTS device_identity_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_username VARCHAR(255) NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    device_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    registration_id BIGINT NOT NULL,
    identity_key TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_username, device_id)
);

CREATE TABLE IF NOT EXISTS device_signed_pre_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_username VARCHAR(255) NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    device_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    key_id BIGINT NOT NULL,
    public_key TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_username, device_id)
);

CREATE TABLE IF NOT EXISTS device_one_time_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_username VARCHAR(255) NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    device_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    key_id BIGINT NOT NULL,
    public_key TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_username, device_id, key_id)
);
