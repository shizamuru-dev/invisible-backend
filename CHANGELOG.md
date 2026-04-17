# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **E2EE Key Backup (Secure Vault):** Added `/keys/backup` endpoints (`POST`, `GET`, `DELETE`) to allow users to securely backup and recover their locally-encrypted cryptographic keys and message histories via a master password/recovery phrase.
- **Read Receipts:** Added support for `ReadReceipt` messages. When a recipient opens the chat, the client sends a `ReadReceipt` which is then forwarded to the original sender (or queued if the sender is offline).
- **Database Migrations:** Introduced `sqlx-cli` migrations (`migrations/`) instead of manual table creation in code, making schema evolutions safer.
- **Configuration Management:** Migrated to `figment` for robust configuration loading. Configurations can now be set via `.env` or direct environment variables using the `AppConfig` struct.
- **Docker & Deployment:** Eliminated hardcoded addresses/ports across the codebase. `docker-compose.yml` was completely refactored to support environment variables with fallbacks (via `.env`), enabling seamless integration into various deployment environments.
- GitHub Actions workflow (`release.yml`) for automated building of Docker images (`ghcr.io`) and creating GitHub Releases with compiled binaries.
- Formal `CHANGELOG.md` file to track project history.

### Changed
- **Sessions & JWT:** Migrated to an Access/Refresh token architecture for improved performance and security.
  - Short-lived stateless Access tokens (15 minutes) are used for HTTP and WebSocket authentication, removing the database bottleneck on every request.
  - Refresh tokens are securely stored in the database and used via `POST /api/auth/refresh` to obtain new Access tokens.
  - Sessions track device information (`device_name`, `device_model`, `platform`, `hwid`) and `last_accessed_at` timestamps.
  - Old or inactive sessions (older than 30 days) are automatically pruned on new logins, and duplicate sessions from the same `hwid` are prevented.
  - Added `/api/auth/logout`, `GET /api/sessions`, and `DELETE /api/sessions/{session_id}` HTTP endpoints for viewing and remote revocation of active sessions.
  - Revoking a session immediately invalidates its Refresh token, requiring re-authentication once the short-lived Access token expires.

## [0.1.0] - 2026-04-15

### Added
- Initial setup of the `invisible-backend` project.
- **Relay Service:** Real-time bidirectional WebSocket communication.
- **API Service:** User registration and JWT-based authentication.
- **Redis Pub/Sub Integration:** Horizontal scalability and inter-node message routing.
- **PostgreSQL Integration:** Reliable offline message queuing.
- **MinIO (S3) Integration:** Secure, out-of-band file transfers via 5-minute presigned URLs.
- **Online Presence Tracking:** `WatchPresence` and `PresenceUpdate` events for tracking user connection status.
- **Delivery Receipts:** Automatic notifications when a recipient successfully receives a message.
- Project documentation (`docs/api.md`, `docs/overview.md`, `docs/agents.md`).
