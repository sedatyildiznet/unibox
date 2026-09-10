# Unibox

**All your chats. One box.**

[![CI](https://github.com/sedatyildiznet/unibox/actions/workflows/ci.yml/badge.svg)](https://github.com/sedatyildiznet/unibox/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/sedatyildiznet/unibox?include_prereleases&label=release)](https://github.com/sedatyildiznet/unibox/releases)
[![Windows](https://img.shields.io/badge/Windows-10%20%2F%2011%20x64-0078D4)](https://github.com/sedatyildiznet/unibox/actions)

Unibox is a **local-first, open-source universal desktop messenger** designed to bring multiple messaging accounts into one unified inbox without requiring an Unibox cloud account or storing your conversations on an Unibox-operated server.

> **Current version:** `0.2.0`
>
> **Project status:** active development. The Windows desktop application, local control daemon, managed WSL2 runtime, installer pipeline and connector architecture are implemented, but connector provisioning and production release signing are still being completed.

## Download & Windows Setup

### Stable / public installer

Production installers will be published on the GitHub Releases page:

**[Download Unibox for Windows](https://github.com/sedatyildiznet/unibox/releases)**

The intended Windows installation flow is:

```text
Download Unibox Setup.exe
        ↓
Run the installer
        ↓
Launch Unibox
        ↓
Unibox prepares the local WSL2 runtime automatically
```

> There is currently **no published GitHub Release**. Do not treat CI smoke builds as production releases.

### CI test builds

Every successful Windows CI run currently produces two temporary artifacts:

| Artifact | Contents | Purpose |
| --- | --- | --- |
| `unibox-windows-installer-smoke` | NSIS `Setup.exe` installer | Installer testing |
| `unibox-windows-smoke` | `unibox-desktop.exe` | Portable/raw executable testing |

Open the latest successful workflow run and download the artifact from the **Artifacts** section:

**[Windows CI builds](https://github.com/sedatyildiznet/unibox/actions/workflows/ci.yml)**

CI artifacts are temporary and currently use a short retention period. They are unsigned smoke/test builds and Windows may display a SmartScreen warning.

### Installer files in this repository

The Windows installation/bootstrap code lives under:

```text
installer/
└── windows-install.ps1
```

Runtime bootstrap resources are bundled from:

```text
apps/desktop/src-tauri/resources/runtime/
```

The NSIS installer itself is generated during the build and is not committed as a binary to the repository.

Build output:

```text
target/release/bundle/nsis/*.exe
```

Raw desktop executable:

```text
target/release/unibox-desktop.exe
```

## Windows Requirements

For normal end users, the target is:

- Windows 10 or Windows 11 x64
- WSL2 support enabled/available
- Internet access during first-time runtime provisioning
- Enough disk space for the desktop app, WSL2 runtime, Synapse, PostgreSQL and connector data

The production goal is that users **do not need to manually install Docker, edit YAML files, manage bridge bots or understand Matrix**.

## Why Unibox

- **Local-first:** Synapse, connector sessions, databases, media cache, settings and indexes are intended to live on the user's device.
- **No Unibox cloud account:** the default architecture has no central Unibox identity or message-storage service.
- **Multi-account:** multiple accounts per network where supported by the upstream connector.
- **Unified inbox:** All Chats, Unread, Mentions, Archive, account filters and global search in one desktop UI.
- **Connector-driven:** messaging networks are integrated through a managed connector layer.
- **Automatic updates:** desktop releases use Tauri updater artifacts from GitHub Releases; managed runtime and connectors have separate verified update tracks.
- **Private runtime:** local APIs are designed to bind to loopback/private IPC. Federation and public registration are disabled in the default desktop runtime.

## Target Connectors

Unibox is designed to support integrations where technically and legally possible, including:

- WhatsApp
- Telegram
- Signal
- Discord
- Instagram
- Messenger
- Google Messages
- Google Chat
- Slack
- X / Twitter DM
- Bluesky
- LinkedIn
- Google Voice
- Zulip
- IRC
- iMessage on supported Apple-platform configurations

Connector availability varies. Some integrations depend on unofficial or reverse-engineered upstream projects and may break when the source platform changes its protocol or policies.

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
  +-- WSL2 (Windows)
  +-- Synapse
  +-- PostgreSQL
  +-- mautrix / compatible connectors
```

On Windows, the managed runtime is intended to operate behind the normal desktop installer and application UI.

## Build From Source

### Requirements

- Node.js 22+
- pnpm 9+
- Rust stable
- Tauri 2 prerequisites
- WSL2 for Windows runtime development/testing

Clone and install dependencies:

```bash
git clone https://github.com/sedatyildiznet/unibox.git
cd unibox
pnpm install --frozen-lockfile
```

Run validation:

```bash
pnpm check
pnpm build
cargo check --workspace --locked
cargo test --workspace --locked
```

Run development mode:

```bash
pnpm dev
```

Build the raw Windows executable:

```bash
pnpm tauri build --no-bundle
```

Build the NSIS Windows setup package:

```bash
pnpm tauri build --bundles nsis --config src-tauri/tauri.ci.conf.json
```

## Release Process

The release workflow is triggered by version tags matching:

```text
v*
```

Example:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The GitHub Actions release workflow performs validation, builds the Windows application and prepares a GitHub Release through `tauri-action`.

### Important: updater signing

Production releases are intentionally blocked until the updater signing configuration is completed.

The placeholder public key in:

```text
apps/desktop/src-tauri/tauri.conf.json
```

must be replaced, and these secrets must be configured in GitHub Actions:

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Never commit the private updater key to the repository.

## Privacy

Unibox itself is designed not to upload message history to an Unibox-operated cloud. Connected services still receive network traffic according to their own protocols and policies, and GitHub may be contacted for update checks.

See [docs/PRIVACY.md](docs/PRIVACY.md).

## Security

The security baseline includes least-privilege Tauri capabilities, localhost-only services, signed application updates, pinned connector artifacts, cryptographic hash verification, automatic connector rollback, redacted diagnostics and migration-safe backups.

See [SECURITY.md](SECURITY.md) and [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

## Repository Structure

```text
.github/workflows/   GitHub Actions CI and release pipelines
apps/desktop/        Tauri + React desktop application
crates/              Rust workspace / backend components
docs/                Architecture, privacy and technical documentation
installer/           Windows installation/bootstrap helpers
scripts/             Build and verification scripts
```

## License

Unibox is intended to be distributed under **AGPL-3.0-or-later**. Bundled and upstream components retain their own licenses.

See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).