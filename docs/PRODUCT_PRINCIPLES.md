# Product principles

## One inbox, many accounts

Unibox is built around a unified conversation list that can combine multiple accounts from multiple supported services. Service and account context remain visible so users can always tell where a conversation belongs.

## Familiar, not copied

The interaction model intentionally follows familiar universal-messenger patterns: a service rail, conversation list, focused chat view, global search, unread filters, account controls and native notifications. The product should feel immediately understandable to people who have used modern messengers, while keeping Unibox's own visual identity and implementation.

## Local by default

The user's local machine is the primary home of Unibox application data. Normal operation does not depend on an Unibox-hosted account or message database.

## Hide infrastructure complexity

Users should not need to know what Matrix, Synapse, appservices, provisioning APIs or bridge registration files are. Installation, upgrades, reconnects, health checks and recovery belong to the runtime manager and graphical interface.

## Capability-driven UI

Different messaging networks support different features. The UI must expose only the capabilities supported by the active connector and conversation instead of pretending every network behaves identically.

## Safe updates

Application and connector updates must be integrity-checked, fail safely and support rollback where practical. Production desktop updates must be cryptographically signed.
