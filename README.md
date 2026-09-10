# Unibox

**All your chats. One box.**

[![CI](https://github.com/sedatyildiznet/unibox/actions/workflows/ci.yml/badge.svg)](https://github.com/sedatyildiznet/unibox/actions/workflows/ci.yml)
[![Native all-services](https://github.com/sedatyildiznet/unibox/actions/workflows/native-v050-all-services-release.yml/badge.svg?branch=native-runtime)](https://github.com/sedatyildiznet/unibox/actions/workflows/native-v050-all-services-release.yml)
[![Windows](https://img.shields.io/badge/Windows-10%20%2F%2011%20x64-0078D4)](https://github.com/sedatyildiznet/unibox/releases)

Unibox is a local-first Windows desktop messenger built with Tauri, React and a Rust native runtime supervisor. Its Windows runtime is bundled with the application: users do not install or manage Matrix, Mautrix, Python, WSL, Docker or a database server themselves.

> **Current native release line:** `0.5.0`
>
> **Final all-services tag:** `v0.5.0-native-all`
>
> A release with that tag is created only when the all-services Windows gate succeeds. Until then, do not treat a development artifact as a supported release.

## Download and Windows setup

The final Windows package is a single NSIS installer:

```text
Unibox-Native-Setup.exe
```

Release page:

**[Unibox releases](https://github.com/sedatyildiznet/unibox/releases)**

Normal installation is intentionally simple:

```text
Download Unibox-Native-Setup.exe
        ↓
Run the installer
        ↓
Launch Unibox
        ↓
The bundled native runtime starts locally
        ↓
Add a service and sign in
```

No WSL installation, Docker Desktop, Ubuntu distribution, Hyper-V provisioning, PowerShell command, terminal command, Python installation or reboot is part of the normal Unibox setup flow.

Development builds are currently unsigned. Windows SmartScreen may therefore warn before running a development installer. A SmartScreen warning is not part of the runtime bootstrap and does not mean WSL or another dependency is required.

## Native Windows architecture

```text
Unibox Desktop
Tauri + React
      |
      v
Rust NativeRuntimeManager
      |
      +-- tuwunel.exe
      |     local Matrix homeserver
      |
      +-- mautrix-*.exe
            native Windows connectors
```

Tuwunel and connector processes bind to localhost. Federation and public Matrix registration are disabled in the desktop runtime.

The application runtime data is stored under:

```text
%LOCALAPPDATA%\app.unibox.desktop\runtime-native\
```

That directory contains the local Matrix database, generated appservice registrations, connector configuration/session databases, local secrets and runtime logs. Installer resources and user data are deliberately separate.

## Services

`registry/stable.json` is the connector catalog and remains the source of truth, but the UI does not blindly expose everything in the catalog.

A service appears in **Add Service** only when the current installer actually contains a supported native adapter and its required runtime files. Telegram additionally requires Unibox application-level credentials to be embedded at build time; Signal requires its native FFI DLL.

The `v0.5.0-native-all` release gate requires all of these target services to pass before the tag may be published:

- WhatsApp
- Telegram
- Signal
- Discord
- Instagram
- Facebook Messenger
- Google Messages
- Google Chat
- Slack
- X / Twitter
- Bluesky
- LinkedIn
- Google Voice
- Zulip
- IRC

If even one required target fails its Windows build/runtime gate, the all-services release is not created. This list is therefore a release target, not a claim that an un-gated development build supports every item.

## Authentication behavior

### WhatsApp

WhatsApp uses the Mautrix BridgeV2 provisioning flow. Unibox renders the QR code in-app and keeps the `display_and_wait` request open while the phone confirms pairing.

### Telegram

End users are never asked for Telegram API ID/API hash or sent to `my.telegram.org`.

The user flow is:

```text
phone number
    ↓
verification code
    ↓
2FA password if required
    ↓
connected
```

The application-level Telegram API credentials are injected only by GitHub Actions using the `UNIBOX_TELEGRAM_API_ID` and `UNIBOX_TELEGRAM_API_HASH` repository secrets. The all-services release fails if those secrets are missing or invalid.

### Legacy connectors

Discord and Google Chat currently use their upstream legacy Mautrix adapters. They are still packaged as local Windows executables; Google Chat's Python dependencies are frozen into its PyInstaller executable so users do not install Python.

## Upgrade behavior

Native binaries are immutable installer resources. The `0.5.0` line uses:

```text
apps/desktop/src-tauri/resources/native-v050/
```

A later release uses a new resource slot rather than overwriting running connector files in place.

Before install/update, the NSIS hook stops the desktop app, Tuwunel and bundled connector processes. User state under `%LOCALAPPDATA%` is not deleted. This avoids locked-file upgrade failures while preserving local sessions and message state.

## Build from source

### Requirements

- Node.js 22+
- pnpm 9+
- Rust stable
- Tauri 2 Windows build prerequisites

Runtime dependencies such as Tuwunel and Mautrix are produced by CI for release builds. End users do not need Go, Rust, Python or C/C++ build tools.

Clone and validate:

```bash
git clone https://github.com/sedatyildiznet/unibox.git
cd unibox
git switch native-runtime
pnpm install --frozen-lockfile
pnpm check
cargo test --workspace --locked
```

Build an NSIS package after the native resource slot has been populated:

```bash
pnpm tauri build --bundles nsis --config src-tauri/tauri.ci.conf.json
```

## Release gate

The canonical Windows all-services workflow is:

```text
.github/workflows/native-v050-all-services-release.yml
```

It is intentionally heavyweight and is not run on every source commit. The final release path must validate the application source, build the Windows runtime, build every required connector, generate connector configs and appservice registrations, start the native runtime, verify local connector ports and BridgeV2 login-flow endpoints, build the NSIS installer, generate SHA-256 sums, and only then publish `v0.5.0-native-all`.

Expected release assets:

```text
Unibox-Native-Setup.exe
SHA256SUMS.txt
```

The release target must be the exact Git commit used for the build.

## Privacy

Unibox does not require an Unibox-operated cloud account. Local Matrix state, connector sessions, media/cache data and local credentials stay on the device by default. Connected third-party services still receive traffic according to their own protocols and policies.

See [docs/PRIVACY.md](docs/PRIVACY.md).

## Security

The native runtime uses loopback-only services, generated local secrets, restricted connector exposure, immutable runtime resource slots and release-time validation.

See [SECURITY.md](SECURITY.md) and [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

## Repository structure

```text
.github/workflows/                 CI and release gates
apps/desktop/                      Tauri + React desktop app
apps/desktop/src-tauri/resources/  versioned native runtime slots
crates/                            Rust workspace
docs/                              architecture and security docs
registry/stable.json               connector catalog
```

## License

Unibox is intended to be distributed under **AGPL-3.0-or-later**. Bundled and upstream components retain their own licenses.

See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
