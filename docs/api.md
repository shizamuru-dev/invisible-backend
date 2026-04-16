# API Documentation

## WebSocket API

**URL:** `ws://localhost:3030/ws`

### Connection

To connect, you must provide a valid JWT token in the `token` query parameter.
`ws://localhost:3030/ws?token=eyJhbGci...`

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
  "password": "supersecretpassword123"
}
```

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

### Sessions

All session endpoints require authentication with a valid Access token.

#### `GET /api/sessions`
Retrieves a list of all active sessions for the current user.

**Responses:**
- `200 OK`:
  ```json
  [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "device_name": "Alice's iPhone",
      "device_model": "iPhone 13 Pro",
      "platform": "iOS",
      "last_accessed_at": "2026-04-16T12:00:00Z"
    }
  ]
  ```

#### `DELETE /api/sessions/{session_id}`
Revokes a specific session by its ID, immediately invalidating its refresh token.

**Responses:**
- `200 OK`: Successfully revoked.
- `404 Not Found`: Session not found or belongs to another user.

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
