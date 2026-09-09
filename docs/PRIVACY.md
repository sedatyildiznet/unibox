# Privacy Model

Unibox is local-first. The default product does not require an Unibox account and does not depend on an Unibox-operated message-storage service.

## Stored locally

The architecture intends to keep Synapse data, connector databases and sessions, local account metadata, media cache, search indexes, preferences, logs and user-created backups on the user's device.

## Network traffic

Local-first does not mean offline. Connectors communicate with the messaging services selected by the user, and those services receive traffic according to their own protocols and policies. GitHub may be contacted for update checks.

## Telemetry

The default architecture does not require message telemetry or product analytics. Any future telemetry must be separately documented, must never include message content or credentials, and should be opt-in unless a compelling operational need is established.

## Backups

Backups can contain highly sensitive session and message data. Encrypted export is required before backup/restore is considered production-ready.