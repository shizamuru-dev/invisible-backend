# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- GitHub Actions workflow (`release.yml`) for automated building of Docker images (`ghcr.io`) and creating GitHub Releases with compiled binaries.
- Formal `CHANGELOG.md` file to track project history.

## [0.1.0] - 2026-04-15

### Added
- Initial setup of the `invisible-backend` project.
- **Relay Service:** Real-time bidirectional WebSocket communication.
- **Auth Service:** User registration and JWT-based authentication.
- **Redis Pub/Sub Integration:** Horizontal scalability and inter-node message routing.
- **PostgreSQL Integration:** Reliable offline message queuing.
- **MinIO (S3) Integration:** Secure, out-of-band file transfers via 5-minute presigned URLs.
- **Online Presence Tracking:** `WatchPresence` and `PresenceUpdate` events for tracking user connection status.
- **Delivery Receipts:** Automatic notifications when a recipient successfully receives a message.
- Project documentation (`docs/api.md`, `docs/overview.md`, `docs/agents.md`).
