# Security Model

Required defaults:

- loopback/private IPC only
- PostgreSQL not exposed externally
- Synapse public registration disabled
- Matrix federation disabled
- connector provisioning APIs not exposed externally
- tokens/cookies/QR secrets redacted from logs
- diagnostics safe to attach after automatic redaction
- signed application updates
- pinned and hashed runtime/connector binaries
- atomic connector update plus health check plus rollback
- local-only connector data by default

OS-backed secure storage should be used for secrets where practical. Bridge databases may necessarily contain service session material and must be restricted to the current user/runtime.