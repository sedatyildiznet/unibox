# Versioning

Unibox uses semantic versioning for desktop releases.

- Patch: compatible fixes and reliability improvements.
- Minor: new user-facing features, connectors or compatible runtime changes.
- Major: intentionally incompatible desktop/runtime or persisted-data changes.

Connector versions are tracked independently from the Unibox desktop version because upstream bridge releases can change on their own schedule.

A connector update must not require pretending that the desktop application itself has a new version. Runtime migrations should record their own schema/version state and be reversible where practical.
