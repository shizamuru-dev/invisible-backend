# Overview

`invisible-backend` is a scalable real-time chat backend.

## Key Features

- **WebSockets:** Provides real-time bidirectional communication.
- **Redis Pub/Sub:** Enables horizontal scalability for routing messages between different server instances.
- **Redis Streams & Batching:** Background workers handle asynchronous batch insertion into PostgreSQL to prevent database locks and maximize Write IOPS under high load.
- **Redis Caching:** Session validation is cached in Redis to minimize synchronous database round-trips upon connection.
- **PostgreSQL:** Reliable message history storage and offline message queuing.
- **MinIO (S3):** Used for out-of-band file transfers to prevent WebSocket memory bloat.

## Workspace Structure

The project is organized into multiple crates:

- `relay`: The WebSocket server handling real-time connections, message routing, Redis Stream publishing, and delivery receipts.
- `api`: HTTP API service handling authentication, presigned URLs for MinIO, user management, and the asynchronous Database Worker.
- `shared`: Shared models, database connections, and repository patterns used across services.

## Prerequisites & Setup

To start the infrastructure, run:

```bash
docker-compose up -d postgres redis minio minio-init loki grafana
```

This will start PostgreSQL, Redis, MinIO, Grafana Loki (for log aggregation), and Grafana.

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
