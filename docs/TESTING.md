# Testing strategy

## Continuous integration

Every important change should pass the Windows CI pipeline before entering `main`. CI validates the connector registry, runtime script syntax, TypeScript, the production frontend, the Rust workspace, Rust tests and a release-mode Tauri executable build.

## Clean Windows acceptance test

Before a stable public release, test on a clean supported Windows installation with no pre-existing Unibox runtime:

1. Launch the installer/application.
2. Provision the managed WSL2 runtime.
3. Verify PostgreSQL and Synapse health.
4. Restart Windows and verify runtime recovery.
5. Install at least one BridgeV2 connector and complete authentication.
6. Send and receive text and media.
7. Add a second account where supported.
8. Exercise connector restart/update/rollback.
9. Verify app restart preserves sessions.
10. Verify uninstall/backup behavior according to the release policy.

## Connector acceptance

Each connector receives its own capability matrix. A connector should not be promoted to Stable based only on successful installation; login, inbound/outbound messages, media, reconnect and restart persistence must be tested.

## Upstream-change testing

Because third-party services can change protocols independently of Unibox, connector health needs to be revalidated when upstream bridge versions change even if the desktop code does not.
