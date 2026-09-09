# Security Policy

Unibox is a local-first desktop application and treats remote-service sessions and message history as sensitive data.

## Baseline

- Local runtime services bind to loopback/private IPC only.
- Public Matrix registration and federation are disabled by default.
- Secrets, cookies, QR payloads and access tokens must not be written to normal logs.
- Application updates must be signed and verified before installation.
- Connector packages must be pinned, checksummed and rollback-capable.
- Diagnostics must redact sensitive values before export.
- The Tauri webview receives only the capabilities required by the desktop UI.

## Reporting

Do not post working exploits, session tokens, credentials or private message data in public issues. Use a private GitHub Security Advisory once enabled, or contact the repository owner privately.

See `docs/SECURITY.md` and `docs/THREAT_MODEL.md`.