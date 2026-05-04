<div align="center">

# 👻 invisible-backend

### *The server that knows everything about you — and forgets it immediately.*

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-CE422B?logo=rust)](https://www.rust-lang.org)
[![Axum](https://img.shields.io/badge/Framework-Axum-blue)](https://github.com/tokio-rs/axum)
[![PostgreSQL](https://img.shields.io/badge/Database-PostgreSQL-336791?logo=postgresql)](https://www.postgresql.org)
[![Redis](https://img.shields.io/badge/Cache-Redis-DC382D?logo=redis)](https://redis.io)
[![Docker](https://img.shields.io/badge/Deploy-Docker-2496ED?logo=docker)](https://www.docker.com)

</div>

---

## 🤔 What is this?

**invisible-backend** is the server side of [Invisible](https://github.com/Glitched-Developers/invisible) — the messenger that takes privacy so seriously it considered not storing your messages at all. (We compromised: we store them encrypted. The server literally cannot read them. Scout's honor.)

Built entirely in **Rust** (because we enjoy pain *and* memory safety), it splits into two services:

- **`api`** — HTTP REST service handling auth, key distribution, and file uploads. The responsible adult in the room.
- **`relay`** — WebSocket server that routes encrypted blobs between clients at the speed of light, while understanding absolutely nothing about their content. A very fast, very blind courier.

---

## ✨ Features

- 🔒 **End-to-end encryption (Signal Protocol)** — X3DH + Double Ratchet. We went full cryptography nerd. No regrets.
- ⚡ **Real-time messaging via WebSockets** — because HTTP polling is a war crime
- 📁 **Out-of-band file transfer via MinIO** — files go straight to S3-compatible storage, encrypted by the client before upload
- 🔑 **Key distribution server (PKI)** — manages identity keys, signed pre-keys, and one-time keys (OTK). When OTKs run out, things get slightly less secure. Replenish them.
- 📬 **Offline message queuing** — messages for offline users are patiently waiting in PostgreSQL, like a very secure voicemail
- 📊 **Horizontal scaling via Redis Pub/Sub** — spin up more relay nodes, Redis handles the routing gossip
- 🔐 **Secure Key Vault** — encrypted backups of E2EE keys, stored server-side as blobs the server cannot open
- 🩺 **Delivery & read receipts** — so you can no longer pretend you didn't see the message

---

## 🛠️ Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | [Rust](https://www.rust-lang.org) (stable) |
| HTTP framework | [Axum 0.8](https://github.com/tokio-rs/axum) |
| Async runtime | [Tokio](https://tokio.rs) |
| Database | [PostgreSQL 16](https://www.postgresql.org) via [sqlx](https://github.com/launchbadge/sqlx) |
| Cache & Pub/Sub | [Redis 7](https://redis.io) |
| File storage | [MinIO](https://min.io) (S3-compatible) |
| Auth | JWT ([jsonwebtoken](https://github.com/Keats/jsonwebtoken)) + Argon2 password hashing |
| Crypto | Signal Protocol — X3DH · Double Ratchet · AES-256-GCM |
| Deployment | Docker + Docker Compose |

> **A note on the crypto:** Yes, X3DH for key agreement, Double Ratchet for forward secrecy, AES-256-GCM for file encryption. We did not hold back. The server is a blind relay that routes ciphertext and asks no questions.

---

## 🚀 Getting Started

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and [Docker Compose](https://docs.docker.com/compose/)
- That's it. Really.

### Quick Start (Docker)

```bash
# 1. Clone the repo
git clone https://github.com/Glitched-Developers/invisible-backend.git
cd invisible-backend

# 2. Copy and configure environment variables
cp .env.example .env
# Open .env and fill in your secrets.
# JWT_SECRET especially — don't leave it as the default. We're begging you.

# 3. Launch everything
docker compose up -d
```

This single command builds the Rust services and starts PostgreSQL, Redis, MinIO, the API server, and the Relay — all at once, like a very organized magician.

### Container Management

```bash
docker compose up -d        # Start all services in the background
docker compose logs -f      # Watch logs scroll by in real time
docker compose down         # Stop and remove all containers
```

### Local Development (without Docker)

If you already have PostgreSQL, Redis, and MinIO running locally:

```bash
cargo run --bin api         # Start the HTTP REST server (default port: 3001)
cargo run --bin relay       # Start the WebSocket server (default port: 3030)
```

---

## ⚙️ Configuration

The project uses environment variables for configuration. Copy `.env.example` to `.env` and fill it in.

Key variables:

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis connection string |
| `JWT_SECRET` | Secret key for signing JWT tokens. **Change this in production. Seriously.** |
| `API_PORT` | HTTP API port (default: `3001`) |
| `RELAY_PORT` | WebSocket relay port (default: `3030`) |
| `S3_ENDPOINT` | MinIO/S3 endpoint URL |
| `S3_ACCESS_KEY` | S3 access key |
| `S3_SECRET_KEY` | S3 secret key |
| `S3_BUCKET` | S3 bucket name for file uploads |

Database migrations live in `migrations/` and are applied automatically at service startup via `sqlx`. No manual `ALTER TABLE` archaeology required.

---

## 🧪 Testing

### Rust integration tests

```bash
cargo test --workspace -- --test-threads=1
```

*(Single-threaded because the tests share a database. They're social but not *that* social.)*

### Node.js API & WebSocket tests

Requires all services to be running:

```bash
cd test_tool && npm install
API_URL=http://localhost:3001 RELAY_WS_URL=ws://localhost:3030 npm run test:all
```

CI runs `fmt`, `clippy`, and all test levels automatically on every pull request. If it's red, it's not going in.

---

## 📚 Documentation

Detailed documentation lives in the `docs/` directory:

- [Architecture Overview & Setup](docs/overview.md)
- [API Reference](docs/api.md)

---

## 🤝 Contributing

Found a bug? Have a performance idea? Want to add *yet another* cryptographic primitive?  
Pull requests are welcome. Please open an issue first so we can argue about it constructively.

1. Fork the repo
2. Create a feature branch (`git checkout -b feat/my-cool-thing`)
3. Commit your changes (`git commit -m 'Add my cool thing'`)
4. Push and open a PR

---

## 👥 Authors & Contributors

| Role | Person |
|------|--------|
| Organization | [@Glitched-Developers](https://github.com/Glitched-Developers) |
| Core team | [Shizamuru](https://github.com/shizamuru-dev) · [VladN13](https://github.com/VladN13) · [vovakovtyn2008-oss](https://github.com/vovakovtyn2008-oss) |

---

## 📄 License

Licensed under the [Mozilla Public License 2.0](LICENSE).  
TL;DR: open source, share your improvements, don't lock it up as proprietary. The lawyers insisted we mention that.

---

<div align="center">

*Made with ☕, 🦀 Rust, and an unhealthy obsession with cryptography.*

*The frontend lives here: **[invisible](https://github.com/Glitched-Developers/invisible)***

</div>
