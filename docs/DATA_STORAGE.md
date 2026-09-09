# Data storage

Unibox is designed so that persistent application state is stored on the user's own device.

## Windows desktop data

The Tauri application uses the operating system's per-user application-data location for desktop state and for the managed runtime root.

## Managed WSL2 runtime

The `UniboxRuntime` distribution contains:

- PostgreSQL data
- Synapse configuration and database access state
- local Matrix session metadata
- connector databases
- connector authentication/session state
- connector configuration
- runtime logs

The runtime is not intended to expose these services to the public network.

## Media and history

Synced conversation metadata and media may consume substantial disk space. Storage controls should eventually allow users to inspect cache size, clear non-essential cached media and create/restore supported backups without exposing raw infrastructure details.

## Secrets

Remote messaging credentials and session tokens are sensitive local data. They must never be included in ordinary logs, telemetry or unredacted diagnostic exports.
