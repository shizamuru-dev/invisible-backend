ALTER TABLE device_one_time_keys ADD COLUMN IF NOT EXISTS consumed BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_device_one_time_keys_not_consumed
    ON device_one_time_keys(device_id)
    WHERE consumed = FALSE;
