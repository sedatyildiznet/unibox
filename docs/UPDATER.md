# Update architecture

Unibox has two update domains: the desktop application and messaging connectors.

## Desktop application

The desktop application uses the Tauri updater. Production releases are published through GitHub Releases and must be cryptographically signed.

Update flow:

1. The user selects **Check for updates** or the application performs an allowed update check.
2. Unibox requests release metadata from the configured GitHub release endpoint.
3. Tauri verifies the release signature against the public key embedded in the application.
4. The update is downloaded and installed only after successful verification.
5. Unibox relaunches into the new version.

Required GitHub Actions secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

The private key must never be committed or included in application artifacts.

## Connector updates

Connectors are independently updateable so upstream protocol fixes can ship without requiring a full desktop release.

Connector update flow:

1. Resolve the approved upstream release from the connector registry.
2. Download the expected platform asset.
3. Verify upstream digest/release metadata.
4. Stop the connector cleanly.
5. Preserve the previous executable/configuration for rollback.
6. Install the new connector.
7. Start it and run a health check.
8. Roll back to the previous known-good build when the update cannot start successfully.

Unibox does not silently execute unverified connector binaries.
