# Contributing

Changes should favor reliability, explicit failure handling, narrow permissions and maintainable adapters over shortcuts.

1. Keep functionality local when it can remain local.
2. Never expose Synapse, PostgreSQL, connector provisioning APIs or `uniboxd` publicly by default.
3. Never log credentials, cookies, QR payloads, access tokens or full private message bodies in diagnostics.
4. Keep service-specific behavior behind connector adapters.
5. Every updater path needs verification and rollback.
6. Preserve a fast unified-inbox workflow without copying proprietary branding, source code or protected visual assets from other products.

PRs should describe implementation, security impact, compatibility impact and test evidence.