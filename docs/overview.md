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

Requires Docker and Docker Compose. Clone the repository and start everything with:

```bash
docker compose up -d
```

This will build Rust services and start all containers: PostgreSQL, Redis, MinIO, API, and Relay.

## Configuration

The project uses `figment` for configuration. Configuration is loaded from:
1. Default values in code (`AppConfig::default()`)
2. Environment variables (overrides defaults)

Environment variables for Docker Compose are defined in `docker-compose.yml`:
- `DATABASE_URL` — PostgreSQL connection string
- `REDIS_URL` — Redis connection string
- `JWT_SECRET` — JWT signing key (change for production!)
- `S3_ENDPOINT`, `S3_ACCESS_KEY`, `S3_SECRET_KEY` — MinIO settings

## Database Migrations

Migrations are in `migrations/` directory and run automatically when services start.

## Running the Services

With Docker:
```bash
docker compose up -d        # Start all services
docker compose logs -f      # Follow logs
docker compose down         # Stop all services
```

Without Docker (local development):
```bash
# Requires PostgreSQL, Redis, MinIO running locally
cargo run --bin relay       # Start relay server
cargo run --bin api         # Start API server
```

## Testing

**Rust Integration Tests:**
```bash
cargo test --workspace -- --test-threads=1
```

**Node.js API Tests** (requires running services):
```bash
cd test_tool && npm install
API_URL=http://localhost:3001 RELAY_WS_URL=ws://localhost:3030 npm run test:all
```

**GitHub Actions CI** runs both on every PR (fmt → clippy → test → integration).
