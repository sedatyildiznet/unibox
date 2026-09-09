# Local-first data model

Unibox does not require a central Unibox account or Unibox-hosted message database. The desktop application is designed so that its persistent application state remains on the user's own computer.

## Stored locally

The managed runtime stores the following on the local device:

- Synapse database and Matrix account state
- connector databases and remote-service session state
- message metadata mirrored into the local Matrix homeserver
- downloaded media and caches
- connector configuration and local runtime metadata
- Unibox preferences and diagnostics

On Windows, the managed backend is hosted in the `UniboxRuntime` WSL2 distribution and the desktop application keeps its own application data under the operating system's per-user application data directory.

## Network communication

Local-first does not mean offline. Connectors must communicate with the messaging services the user chooses to connect, and the desktop updater may contact GitHub Releases when the user checks for updates. Unibox itself does not require a hosted Unibox message relay for normal operation.

## Security boundaries

- Synapse binds its client listener to localhost.
- Public Matrix registration is disabled.
- Federation is not used by the managed local runtime.
- Connector processes are isolated from the desktop UI behind the runtime manager.
- Connector downloads are validated against upstream release metadata before execution.
- Production application updates are signature-verified by Tauri.

Users should still protect their Windows account and disk because connector session credentials necessarily exist on the local device in order to keep accounts signed in.
