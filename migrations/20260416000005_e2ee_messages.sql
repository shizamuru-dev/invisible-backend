CREATE TABLE message_ciphertexts (
    id UUID PRIMARY KEY,
    message_id VARCHAR(255) NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    device_id UUID NOT NULL,
    ciphertext TEXT NOT NULL,
    signal_type INT NOT NULL
);

CREATE INDEX idx_message_ciphertexts_message_id ON message_ciphertexts(message_id);
CREATE INDEX idx_message_ciphertexts_device_id ON message_ciphertexts(device_id);
