# Support and diagnostics

Unibox should make failures actionable without requiring users to understand Matrix internals.

Diagnostic surfaces should report:

- desktop version
- runtime version/state
- WSL availability
- PostgreSQL health
- Synapse health
- connector versions and health
- storage usage
- recent redacted errors

Diagnostic exports must remove access tokens, passwords, cookies and connector session secrets before leaving the local device.
