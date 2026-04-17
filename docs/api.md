# API Documentation

## WebSocket API

**URL:** `ws://localhost:3030`

### Connection

To connect, provide a valid JWT token in the `token` query parameter:
`ws://localhost:3030?token=eyJhbGci...`

The server decodes the token using the `JWT_SECRET` and extracts the `sub` claim as the user's ID. If the token is missing, invalid, or expired, the WebSocket connection is rejected with `401 Unauthorized`.

### Client -> Server (IncomingMessage)

Messages sent by the client must be JSON strings with the following formats:

#### Text Message
```json
{
  "type": "Text",
  "to": "bob",
  "id": "msg-123",
  "content": "Hello, Bob!"
}
```

#### File Message
```json
{
  "type": "File",
  "to": "bob",
  "id": "msg-file-123",
  "file_name": "image.png",
  "mime_type": "image/png",
  "file_url": "http://localhost:9000/chat-files/image.png"
}
```

#### Encrypted Message (E2EE)
Sends an end-to-end encrypted message. The `ciphertexts` array contains one entry per target device. The server cannot decrypt the content.

```json
{
  "type": "Encrypted",
  "to": "bob",
  "id": "msg-e2ee-456",
  "ciphertexts": [
    {
      "device_id": "550e8400-e29b-41d4-a716-446655440000",
      "signal_type": 3,
      "ciphertext": "BASE64_ENCODED_CIPHERTEXT"
    },
    {
      "device_id": "661f9511-f3ac-52e5-b827-557766551111",
      "signal_type": 1,
      "ciphertext": "BASE64_ENCODED_CIPHERTEXT"
    }
  ]
}
```

`signal_type` values: `3` = PreKey message (first message / new session), `1` = Normal message (established session).

#### Typing Indicator
```json
{
  "type": "Typing",
  "to": "bob"
}
```

#### Read Receipt
```json
{
  "type": "ReadReceipt",
  "to": "bob",
  "message_id": "msg-123"
}
```

#### Watch Presence
```json
{
  "type": "WatchPresence",
  "user_ids": ["bob", "charlie"]
}
```

### Server -> Client (OutgoingMessage)

The server sends messages back to the client in these formats.

#### Text Message
```json
{
  "type": "Text",
  "from": "alice",
  "id": "msg-123",
  "content": "Hello, Bob!"
}
```

#### File Message
```json
{
  "type": "File",
  "from": "alice",
  "id": "msg-file-123",
  "file_name": "image.png",
  "mime_type": "image/png",
  "file_url": "http://localhost:9000/chat-files/image.png"
}
```

#### Encrypted Message (E2EE)
Received when the sender sends an encrypted message targeting the current user's devices.

```json
{
  "type": "Encrypted",
  "from": "alice",
  "id": "msg-e2ee-456",
  "ciphertexts": [
    {
      "device_id": "550e8400-e29b-41d4-a716-446655440000",
      "signal_type": 3,
      "ciphertext": "BASE64_ENCODED_CIPHERTEXT"
    }
  ]
}
```

The client should find the ciphertext entry matching its own `device_id` (from the JWT `session_id`) and decrypt it locally using the Signal Protocol session.

#### Typing Indicator
```json
{
  "type": "Typing",
  "from": "alice"
}
```

#### Delivery Receipt
When a user successfully receives a text or file message, the server immediately sends a delivery receipt to the original sender.

```json
{
  "type": "DeliveryReceipt",
  "to": "bob",
  "message_id": "msg-123"
}
```

#### Read Receipt
When the recipient opens the chat and reads the message, their client should send a ReadReceipt. The server forwards it to the original sender.

```json
{
  "type": "ReadReceipt",
  "from": "alice",
  "message_id": "msg-123"
}
```

#### Presence Update
When users come online or go offline, or when initially requested.

```json
{
  "type": "PresenceUpdate",
  "user_id": "bob",
  "is_online": true
}
```

## HTTP API

**URL:** `http://localhost:3001`

### Authentication

#### `POST /api/auth/register`
Registers a new user in the system.

**Request Body (JSON):**
```json
{
  "username": "alice",
  "password": "supersecretpassword123"
}
```

**Responses:**
- `201 Created`: User created successfully.
- `400 Bad Request`: Username or password missing.
- `409 Conflict`: Username already exists.

#### `POST /api/auth/login`
Authenticates a user and returns a session token. Also registers a new session in PostgreSQL and caches it in Redis.

**Request Body (JSON):**
```json
{
  "username": "alice",
  "password": "supersecretpassword123",
  "device_info": {
    "device_name": "Alice's iPhone",
    "device_model": "iPhone 15 Pro",
    "platform": "iOS",
    "hwid": "hardware-id-abc123"
  }
}
```

`device_info` is optional. If provided, the device is registered with the given metadata (used for E2EE device management).

**Responses:**
- `200 OK`:
  ```json
  {
    "token": "eyJ0eXAi... (JWT Token valid for 7 days)"
  }
  ```
- `401 Unauthorized`: Invalid username or password.

#### `POST /api/auth/logout`
Logs the user out of the current session, immediately invalidating the session in PostgreSQL and deleting it from the Redis cache. Requires Authentication.

**Responses:**
- `200 OK`: Successfully logged out.
- `401 Unauthorized`: Missing, invalid, or expired token.

### E2EE Key Management

All key management endpoints require authentication with a valid JWT token (Bearer header).

#### `POST /keys/upload`
Uploads the current device's cryptographic key bundle. Used for Signal Protocol X3DH key distribution. Should be called after device registration and periodically to replenish one-time keys.

**Request Body (JSON):**
```json
{
  "identity_key": "BASE64_ENCODED_IDENTITY_PUBLIC_KEY",
  "registration_id": 12345,
  "signed_pre_key": {
    "key_id": 1,
    "public_key": "BASE64_ENCODED_SIGNED_PRE_KEY",
    "signature": "BASE64_ENCODED_SIGNATURE"
  },
  "one_time_keys": [
    { "key_id": 1, "public_key": "BASE64_ENCODED_OTK_1" },
    { "key_id": 2, "public_key": "BASE64_ENCODED_OTK_2" }
  ]
}
```

**Responses:**
- `200 OK`: Keys uploaded successfully. Identity and Signed PreKey are upserted; OTKs are inserted (duplicates ignored).
- `400 Bad Request`: Invalid session ID in JWT.
- `401 Unauthorized`: Missing or invalid token.

#### `GET /keys/claim/{username}`
Claims the key bundle for all devices of a given user. Used by the sender to initiate an X3DH handshake with each device. **One-Time Keys are consumed atomically** -- each call pops one OTK per device.

**Response:**
```json
{
  "devices": [
    {
      "device_id": "550e8400-e29b-41d4-a716-446655440000",
      "identity_key": "BASE64...",
      "registration_id": 12345,
      "signed_pre_key": {
        "key_id": 1,
        "public_key": "BASE64...",
        "signature": "BASE64..."
      },
      "one_time_key": {
        "key_id": 1,
        "public_key": "BASE64..."
      },
      "one_time_keys_remaining": 42
    }
  ]
}
```

**Responses:**
- `200 OK`: Key bundles returned.
- `404 Not Found`: No keys found for this user (user has no registered devices).
- `401 Unauthorized`: Missing or invalid token.

**Note:** `one_time_key` may be `null` if all OTKs for a device have been consumed. The client should still proceed with the X3DH handshake using only the Signed PreKey in this case (fallback mode). `one_time_keys_remaining` shows how many OTKs are left for that device (before the current claim). When it reaches 0, client should upload fresh OTKs via `POST /keys/upload`.

### Files

All file endpoints require authentication. You must provide a valid JWT token either:
- In the `Authorization` header as a Bearer token (`Authorization: Bearer <token>`)
- As a query parameter (`?token=<token>`)

#### `GET /files/presign`

Provides temporary, secure URLs for uploading files to MinIO.
The client retrieves a presigned URL, uploads the file directly to MinIO (bypassing the server), and then sends a WebSocket `File` message with the resulting URL.

**Note:** Both the returned `upload_url` and the actual MinIO download links behind the `download_url` are only valid for **5 minutes**.

#### Query Parameters
- `file_name` (string, required): The name of the file to be uploaded.
- `mime_type` (string, required): The MIME type of the file.

#### Response
**Status:** 200 OK
```json
{
  "upload_url": "http://localhost:9000/uploads/... (Presigned URL)",
  "download_url": "http://localhost:3001/files/download/123e4567-e89b-12d3-a456-426614174000",
  "file_id": "123e4567-e89b-12d3-a456-426614174000"
}
```

#### Upload Flow
1. Client requests `GET /files/presign?file_name=photo.jpg&mime_type=image/jpeg` (with token).
2. Client performs a `PUT` request to `upload_url` with the file content.
3. Client sends a WebSocket `File` message containing the `download_url`.

#### `GET /files/download/{file_id}`

Securely downloads a file by its ID. Requires authentication (Bearer header or `?token=` query param).
It automatically redirects the client to a 5-minute temporary presigned MinIO URL for the requested file.

### Key Backup (Secure Vault)

These endpoints allow clients to securely backup and restore their cryptographic keys (and optionally message history). The backend does not know the encryption password or recovery phrase, so all data must be encrypted client-side before uploading.

#### `POST /keys/backup`
Uploads or overwrites the user's encrypted key vault.

**Request Body (JSON):**
```json
{
  "encrypted_vault": "BASE64_ENCODED_CIPHERTEXT",
  "salt": "BASE64_ENCODED_SALT",
  "mac": "BASE64_ENCODED_MAC"
}
```

**Responses:**
- `200 OK`: Backup saved successfully.
- `401 Unauthorized`: Missing or invalid token.

#### `GET /keys/backup`
Retrieves the user's encrypted key vault.

**Response:**
```json
{
  "encrypted_vault": "BASE64_ENCODED_CIPHERTEXT",
  "salt": "BASE64_ENCODED_SALT",
  "mac": "BASE64_ENCODED_MAC",
  "updated_at": "2026-04-17T20:40:00Z"
}
```

**Responses:**
- `200 OK`: Backup returned.
- `404 Not Found`: No backup exists for this user.
- `401 Unauthorized`: Missing or invalid token.

#### `DELETE /keys/backup`
Deletes the user's encrypted key vault from the server.

**Responses:**
- `200 OK`: Backup deleted successfully.
- `401 Unauthorized`: Missing or invalid token.
