# Overview

`invisible-backend` is a scalable real-time chat backend.

## Key Features

- **WebSockets:** Provides real-time bidirectional communication.
- **Redis Pub/Sub:** Enables horizontal scalability for routing messages between different server instances.
- **PostgreSQL:** Reliable offline message queuing when recipients are disconnected.
- **MinIO (S3):** Used for out-of-band file transfers to prevent WebSocket memory bloat.

## Workspace Structure

The project is organized into multiple crates:

- `relay`: The WebSocket server handling real-time connections, message routing, and delivery receipts.
- `api`: Service handling apientication, presigned URLs for MinIO, and user management.
- `shared`: Shared models, database connections, and repository patterns used across services.

## Prerequisites & Setup

To start the infrastructure, run:

```bash
docker-compose up -d postgres redis minio minio-init
```

This will start PostgreSQL, Redis, MinIO, and a small init container that creates the necessary bucket in MinIO.

## Running the Services

```bash
# Start the relay server
cargo run --bin relay

# Start the api server
cargo run --bin api
```

## Testing

Integration tests require a running PostgreSQL and Redis instance. Due to how the tables are created, tests must be run with a single thread to avoid database conflicts.

```bash
cargo test --all -- --test-threads=1
```
