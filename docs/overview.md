# Overview

`invisible-backend` is a scalable real-time chat backend.

## Key Features

- **End-to-End Encryption (E2EE):** The server acts as a blind relay and PKI server. Signal Protocol (X3DH + Double Ratchet) for 1-1 chats; Olm/Megolm architecture prepared for future group chats. The server never sees plaintext message content.
- **WebSockets:** Provides real-time bidirectional communication.
- **Redis Pub/Sub:** Enables horizontal scalability for routing messages between different server instances.
- **Redis Streams & Batching:** Background workers handle asynchronous batch insertion into PostgreSQL to prevent database locks and maximize Write IOPS under high load.
- **Redis Caching:** Session validation is cached in Redis to minimize synchronous database round-trips upon connection.
- **PostgreSQL:** Reliable message history storage, offline message queuing, and E2EE key distribution (device keys, signed pre-keys, one-time keys).
- **MinIO (S3):** Used for out-of-band file transfers to prevent WebSocket memory bloat. Files are encrypted client-side before upload (AES-256-GCM with key exchange via Signal Protocol).

## Workspace Structure

The project is organized into multiple crates:

- `relay`: The WebSocket server handling real-time connections, message routing (including E2EE encrypted blobs), Redis Stream publishing, and delivery receipts.
- `api`: HTTP API service handling authentication, E2EE key distribution (`/keys/upload`, `/keys/claim`, `/keys/devices`), presigned URLs for MinIO, user management, and the asynchronous Database Worker.
- `shared`: Shared models (including `DeviceCiphertext`, `IncomingMessage::Encrypted`, etc.), database connections, and repository patterns used across services.

## Prerequisites & Setup

To start the infrastructure, run:

```bash
docker-compose up -d postgres redis minio minio-init
```

This will start PostgreSQL, Redis, and MinIO.

## Configuration

The project uses `figment` for configuration management. Configuration values are loaded from:
1. Default values defined in the code (`AppConfig::default()`).
2. A `.env` file at the root of the workspace.
3. System environment variables (e.g., `DATABASE_URL`, `JWT_SECRET`).

Example `.env` file:
```env
DATABASE_URL=postgres://invisible:password@127.0.0.1:5432/invisible_chat
REDIS_URL=redis://127.0.0.1:6379/
JWT_SECRET=super-secret-key-for-dev
```

## Database Migrations

The project uses `sqlx-cli` for database migrations. The migrations are located in the `migrations/` directory and are automatically applied when the `api` or `relay` services start.

## Running the Services

```bash
# Start the relay server
cargo run --bin relay

# Start the api server
cargo run --bin api
```

## Testing

Integration tests require a running PostgreSQL and Redis instance. They will automatically run the migrations against the test database. Tests should be run with a single thread to avoid data conflicts between concurrent test cases:

```bash
cargo test --all -- --test-threads=1
```
