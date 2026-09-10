# Unibox

**All your chats. One box.**

Unibox is a local-first, open-source desktop universal messenger. It is designed to bring multiple accounts from supported messaging networks into one unified inbox without requiring an Unibox cloud account or storing your conversations on an Unibox-operated server.

> **Status:** engineering preview. Windows packaging and automated checks do not establish production readiness. See [implementation status](docs/IMPLEMENTATION_STATUS.md) and [Windows acceptance](docs/WINDOWS_ACCEPTANCE.md).

## Why Unibox

- **Local-first:** Synapse, connector sessions, databases, media cache, settings and indexes are intended to live on the user's device.
- **No Unibox account required:** the default architecture has no central Unibox identity or message-storage service.
- **Multi-account:** connect multiple accounts per network when the upstream connector supports it.
- **Unified inbox:** modern three-pane messenger workflow with All Chats, Unread, Mentions, Archive, account filters and global search.
- **Connector-driven:** WhatsApp, Telegram, Signal, Discord, Instagram, Messenger, Google Messages, Google Chat, Slack, X/Twitter DM, Bluesky, LinkedIn, Google Voice, Zulip, IRC and platform-specific iMessage are target connectors where technically possible.
- **Automatic updates:** the desktop app uses signed Tauri updater artifacts from GitHub Releases; connectors and the managed runtime have separate verified update tracks.
- **Private runtime:** local APIs are designed to bind to loopback/private IPC only. Federation and public registration are disabled in the default desktop runtime.

## Architecture

```text
Unibox Desktop (Tauri 2 + React + TypeScript)
        |
        | localhost / private IPC
        v
uniboxd (Rust)
        |
        +-- Runtime Manager
        +-- Connector Manager
        +-- Account Manager
        +-- Health Monitor
        +-- Update / Rollback
        +-- Backup / Restore
        |
        v
Managed Local Runtime
  +-- Synapse
  +-- PostgreSQL
  +-- mautrix connectors
```

On Windows, the production target is a managed WSL2 runtime hidden behind the normal desktop installer. Users should not need Docker, YAML editing, bridge bots, or Matrix knowledge.

## Privacy

Unibox itself is designed not to upload message history to an Unibox cloud. Connected services still receive network traffic according to their own protocols and policies, and GitHub may be contacted for update checks. See [docs/PRIVACY.md](docs/PRIVACY.md).

## Security

The security baseline includes least-privilege Tauri capabilities, localhost-only services, signed application updates, pinned connector artifacts, cryptographic hash verification, automatic connector rollback, redacted diagnostics and migration-safe backups. See [SECURITY.md](SECURITY.md) and [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

## Development

Requirements:

- Node.js 20+
- pnpm 9+
- Rust stable
- Tauri 2 prerequisites
- WSL2 for Windows runtime development

```bash
pnpm install
pnpm check
cargo check --workspace
pnpm dev
```

## Release safety

The updater public key placeholder in `apps/desktop/src-tauri/tauri.conf.json` **must be replaced before publishing a release**. The private updater signing key must only exist in trusted local storage and GitHub Actions secrets.

## License

Unibox is intended to be distributed under **AGPL-3.0-or-later**. Bundled/upstream components retain their own licenses. See `THIRD_PARTY_NOTICES.md`.