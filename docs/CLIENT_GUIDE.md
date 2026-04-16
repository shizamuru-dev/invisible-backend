# Client Developer Guide

A guide for developers building client applications that connect to Invisible Backend.

---

## Architecture Overview

```
┌─────────────┐     HTTPS      ┌─────────────┐
│   Client    │◄──────────────►│  API Server │
│ Application │                │  (port 8080)│
└──────┬──────┘                └──────┬──────┘
       │                               │
       │        WebSocket              │
       └──────────────────────────────►│
                                        │
                                   ┌────▼────┐
                                   │  Redis  │
                                   └────┬────┘
                                        │
                                   ┌────▼────┐
                                   │ Postgres│
                                   └─────────┘

       ┌───────────────────────────────▼──────────┐
       │           Relay Server (port 8081)        │
       │  Handles WebSocket connections + Pub/Sub  │
       └───────────────────────────────────────────┘
```

**Key Principles:**
- Backend is **Zero Trust** — server never sees plaintext messages
- All E2EE (End-to-End Encryption) happens client-side
- Server stores only encrypted blobs and cryptographic key material

---

## Authentication

### Registration

```http
POST /api/auth/register
Content-Type: application/json

{
    username: string,      // 3-50 chars, alphanumeric + underscore
    password: string       // min 8 chars
}
```

**Response:** `201 Created` on success, `409 Conflict` if username exists.

### Login

```http
POST /api/auth/login
Content-Type: application/json

{
    username: string,
    password: string,
    device_info?: {        // optional but recommended
        device_name: string,
        device_model: string,
        platform: string,
        hwid: string
    }
}
```

**Response:**
```json
{
    token: string  // JWT, valid for 7 days
}
```

### Using the Token

Include token in `Authorization` header:
```http
Authorization: Bearer <token>
```

For WebSocket connections, pass token as query parameter:
```
ws://host:8081?token=<token>
```

> **Security Note:** Tokens in URLs can leak via logs. Prefer header-based auth for HTTP requests.

---

## End-to-End Encryption (E2EE)

The backend implements the **Signal Protocol** (Olm/Megolm) for E2EE.

### Key Hierarchy

```
Identity Key Pair (Curve25519)
    └── Signed PreKey (Curve25519)
        └── One-Time PreKeys (Curve25519, generated per session)
```

### Key Upload

Upload your device's public keys after login:

```http
POST /keys/upload
Authorization: Bearer <token>
Content-Type: application/json

{
    identity_key: string,          // 32-byte Curve25519 key, Base64-encoded
    registration_id: number,       // 0-16380, client registration identifier
    signed_pre_key: {
        key_id: number,            // 0-1,000,000
        public_key: string,        // 32-byte key, Base64
        signature: string          // 64-byte Ed25519 signature, Base64
    },
    one_time_keys: [
        { key_id: number, public_key: string },  // 32-byte key each
        ...
    ]
}
```

**Limits:**
- Max 100 one-time keys per upload
- Key IDs must be 0-1,000,000
- Registration ID must be 0-16380

### Claiming Keys

Get a recipient's device keys before starting a conversation:

```http
GET /keys/claim/{username}
Authorization: Bearer <token>
```

**Response:**
```json
{
    devices: [
        {
            device_id: string,
            identity_key: string,
            registration_id: number,
            signed_pre_key: {
                key_id: number,
                public_key: string,
                signature: string
            },
            one_time_key: { key_id: number, public_key: string } | null,
            one_time_keys_remaining: number
        }
    ]
}
```

> **Note:** Claimed one-time keys are consumed server-side. The next claim request won't return the same key.

### Device Management

**List your devices:**
```http
GET /keys/devices
Authorization: Bearer <token>
```

**Delete a device:**
```http
DELETE /keys/devices/{device_id}
Authorization: Bearer <token>
```

Returns `200` on success, `403` if device doesn't belong to you.

---

## WebSocket Communication

The Relay server handles real-time message delivery via WebSocket.

### Connection

```
ws://host:port?token=<jwt_token>
```

### Message Types

**Client → Server:**

```javascript
// Send encrypted message
{
    type: 'Encrypted',
    to: 'recipient_username',
    id: 'unique_message_id',           // UUID for deduplication
    ciphertexts: [
        {
            device_id: 'target_device_id',
            signal_type: 3,            // 3 = PreKey message
            ciphertext: 'base64_encrypted_blob'
        }
    ]
}

// Watch for presence changes
{
    type: 'WatchPresence',
    user_ids: ['user1', 'user2']
}

// Send read receipt
{
    type: 'ReadReceipt',
    message_id: 'message_uuid',
    read_at: '2024-04-16T10:30:00Z'
}
```

**Server → Client:**

```javascript
// New message notification
{
    type: 'NewMessage',
    from: 'sender_username',
    id: 'message_id',
    message_type: 'encrypted' | 'text',
    ciphertexts?: [...],              // for encrypted messages
    content?: string,                 // for plaintext messages
    created_at: 'timestamp'
}

// Presence update
{
    type: 'PresenceUpdate',
    user_id: 'username',
    online: boolean,
    last_seen: 'timestamp'
}

// Acknowledgement
{
    type: 'Ack',
    id: 'message_id',
    status: 'delivered' | 'read'
}
```

### Offline Message Handling

If recipient is offline, the server queues messages. They receive queued messages on reconnect.

---

## File Operations

### Presign (get upload URL)

```http
GET /files/presign?file_name=image.png&mime_type=image/png
Authorization: Bearer <token>
```

**Response:**
```json
{
    upload_url: string,
    download_url: string,
    file_id: string
}
```

### Download

```http
GET /files/download/{file_id}
Authorization: Bearer <token>
```

---

## Error Handling

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| 400 | Bad request — invalid input, validation failed |
| 401 | Unauthorized — missing or invalid token |
| 403 | Forbidden — access denied |
| 404 | Not found — resource doesn't exist |
| 409 | Conflict — username exists, duplicate key |
| 500 | Internal server error |

### Error Response Format

```json
{
    error: string  // Human-readable message
}
```

### Key Upload Validation Errors

If key validation fails, you'll receive `400` with details:
- `Invalid identity_key: Invalid key length`
- `Invalid signature: Invalid Base64 encoding`
- `Too many one-time keys`

---

## Client Implementation Checklist

### Authentication
- [ ] Register user with username/password
- [ ] Store JWT token securely
- [ ] Include token in all API requests
- [ ] Handle token refresh before expiration

### E2EE Setup
- [ ] Generate Identity Key Pair on first launch
- [ ] Generate Signed PreKey (rotate periodically)
- [ ] Generate 100+ One-Time PreKeys
- [ ] Upload keys after login
- [ ] Track OTK count, upload more when low (< 20)

### Key Exchange
- [ ] Before sending first message, claim recipient's keys
- [ ] Parse response, build Session for each device
- [ ] Remove consumed OTK from local state
- [ ] Handle `one_time_key: null` (no OTK left for device)

### Messaging
- [ ] Encrypt messages using established sessions
- [ ] Include ciphertext for each target device
- [ ] Generate unique message IDs for deduplication
- [ ] Handle offline delivery via server queue

### Presence
- [ ] Subscribe to presence updates with `WatchPresence`
- [ ] Update UI when users come online/offline
- [ ] Unsubscribe when leaving conversation

---

## Testing Your Integration

### Test Tool

Use `test_tool/` in the repository:

```bash
cd test_tool
npm install
npm run test:all     # Run all tests
npm run test:e2ee    # Test E2EE key exchange
npm run test:api     # Test API endpoints
```

### Manual Testing Flow

1. Register two users (e.g., `alice`, `bob`)
2. Login as both, upload keys
3. Alice claims Bob's keys
4. Alice builds session, sends encrypted message
5. Bob receives via WebSocket or on reconnect

### Key Validation

Test that invalid keys are rejected:
- Wrong length keys → `400 Bad Request`
- Invalid Base64 → `400 Bad Request`
- Keys exceeding limits → `400 Bad Request`

---

## Environment Variables

When connecting to the backend, configure:

| Variable | Default | Description |
|----------|---------|-------------|
| `API_URL` | `http://localhost:8080` | API server base URL |
| `RELAY_WS_URL` | `ws://localhost:8081` | Relay WebSocket URL |

---

## Common Issues

**Q: My claims return empty devices**
A: User hasn't uploaded keys yet. Ensure they call `POST /keys/upload`.

**Q: WebSocket disconnects immediately**
A: Check token validity. Token in query param must be properly URL-encoded.

**Q: OTK claim returns null one_time_key**
A: All OTKs consumed. User needs to upload more keys.

**Q: 401 on API requests**
A: Token expired (7 days) or Authorization header malformed.

**Q: Encrypted message not delivered**
A: Check recipient is online. Messages to offline users are queued.

---

## Protocol Details

### Signal Protocol Implementation

For E2EE, client must implement Olm/Megolm:

- **Olm**: For 1:1 messages (uses Curve25519)
- **Megolm**: For group messages (uses AES-256-GCM + Megolm ratchet)

### Key Generation

```javascript
// Identity Key (generate once, store permanently)
identityKeyPair = generateCurve25519KeyPair()

// Signed PreKey (rotate monthly)
signedPreKey = generateCurve25519KeyPair()
signature = signWithEd25519(signedPreKey.publicKey, identityKeyPair.privateKey)

// One-Time PreKeys (generate 100+, replenish when low)
otks = Array(100).fill(generateCurve25519KeyPair())
```

### Message Encryption Flow

```
1. claim_keys(target_username) → get their DeviceKeys[]
2. for each device:
   - If no session exists, create using identity_key + signed_pre_key + otk
   - Encrypt message using session
3. Send Encrypted WS message with all ciphertexts
```

---

## Security Considerations

- **Never send plaintext to server** — all messages must be encrypted client-side
- **Validate all keys from server** — check signatures, verify key lengths
- **Rotate keys periodically** — replace signed prekeys monthly, replenish OTKs
- **Secure token storage** — don't store in localStorage, use secure enclave
- **Wipe session on logout** — clear all cryptographic material