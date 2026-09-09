# Release Readiness

Unibox is a local-first desktop application. The application, Matrix homeserver state, bridge databases, media cache, sessions and account metadata are designed to remain on the user's own device.

## Automated verification

The Windows CI pipeline verifies:

- connector registry integrity and upstream release metadata
- PowerShell and shell runtime script syntax
- TypeScript type checking
- production frontend build
- Rust workspace compilation
- Rust test suite execution
- Tauri Windows release executable build

## Runtime validation

The managed Windows runtime uses WSL2 and provisions PostgreSQL, Synapse and connector processes locally. A clean-machine WSL2 smoke test must be performed on a real supported Windows installation before a public stable release, because hosted CI environments do not guarantee nested virtualization and systemd behavior identical to an end-user computer.

## Production update signing

Public releases must be signed with the Tauri updater signing key. The private key must never be committed to this repository. Configure these GitHub Actions secrets before publishing production releases:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

The matching public key must be embedded in `apps/desktop/src-tauri/tauri.conf.json`.

Do not publish an unsigned build as a stable release when automatic updates are enabled.
