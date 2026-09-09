# Security model

Unibox is designed as a single-user local desktop application with a managed local messaging runtime.

## Trust model

The Windows user account and local disk are part of the trusted computing base. Unibox stores connector sessions locally because persistent sign-in is impossible without retaining service credentials or tokens somewhere on the user's device.

## Local services

- Synapse is configured for local use and binds its client listener to `127.0.0.1`.
- Public Matrix registration is disabled.
- The managed runtime does not rely on Matrix federation.
- PostgreSQL and connector state remain inside the local managed runtime.
- The normal local Matrix user is not granted homeserver administrator privileges.

## Connector isolation

Connector processes are managed through the runtime layer rather than being controlled directly by arbitrary renderer code. Connector state and databases are separated by connector. Legacy connectors are handled by dedicated adapters instead of being treated as modern BridgeV2 services.

## Supply chain

- Connector registry entries identify upstream projects and expected assets.
- CI validates connector metadata against upstream repositories/releases.
- Downloaded connector artifacts are integrity-checked before execution.
- Desktop production updates use Tauri's signed updater mechanism.
- Signing private keys must only exist in protected release infrastructure and must never be committed.

## Logging and diagnostics

Logs and diagnostic exports should avoid session tokens, passwords and authentication cookies. Future diagnostic tooling must redact secrets before creating support bundles.

## Limitations

Unibox cannot make third-party messaging protocols more secure than the services themselves. Some connectors rely on unofficial client protocols and can be affected by upstream service changes, rate limits, additional verification requirements or account restrictions.
