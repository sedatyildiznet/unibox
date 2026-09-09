# Stable release checklist

- Windows CI passes on the release commit.
- Production frontend and Tauri Windows executable build successfully.
- Managed WSL2 runtime is tested on a clean supported Windows machine.
- PostgreSQL and Synapse survive restart/recovery tests.
- Stable connectors pass login, receive, send, media and reconnect checks.
- Connector update rollback is exercised.
- Tauri updater signing public key is embedded.
- GitHub Actions release signing secrets are configured.
- Release artifacts and checksums are reviewed.
- Privacy, security and known-limitations documentation matches actual behavior.
