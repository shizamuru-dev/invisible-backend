# API Documentation

## WebSocket API

**URL:** `ws://localhost:3030/ws`

### Connection

To connect, you must provide a `user_id` query parameter.
`ws://localhost:3030/ws?user_id=alice`

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

## HTTP API

**URL:** `http://localhost:3001`

### `GET /files/presign`

Provides temporary, secure URLs for uploading files to MinIO.
The client retrieves a presigned URL, uploads the file directly to MinIO (bypassing the server), and then sends a WebSocket `File` message with the resulting URL.

#### Query Parameters
- `file_name` (string, required): The name of the file to be uploaded.
- `mime_type` (string, required): The MIME type of the file.

#### Response
**Status:** 200 OK
```json
{
  "upload_url": "http://localhost:9000/chat-files/... (Presigned URL)",
  "download_url": "http://localhost:9000/chat-files/123e4567-e89b-12d3-a456-426614174000-image.png",
  "file_id": "123e4567-e89b-12d3-a456-426614174000"
}
```

#### Upload Flow
1. Client requests `GET /files/presign?file_name=photo.jpg&mime_type=image/jpeg`.
2. Client performs a `PUT` request to `upload_url` with the file content.
3. Client sends a WebSocket `File` message containing the `download_url`.
