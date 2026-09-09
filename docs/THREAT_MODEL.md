# Threat Model

## Assets

Remote-service sessions, message history, attachments, Matrix credentials, provisioning tokens, local databases, backups and update signing keys.

## Trust boundaries

Desktop webview <-> Tauri shell <-> uniboxd <-> local runtime <-> Synapse/connectors <-> remote services, plus the GitHub Releases update channel.

## Primary threats

Public port exposure, malicious connector updates, log leakage, excessive webview privileges, dependency/supply-chain compromise, database corruption, backup theft and remote-service protocol changes.

## Required mitigations

Loopback/private IPC, least-privilege Tauri capabilities, signed app updates, cryptographic connector verification, atomic replacement/rollback, pre-migration backups, redacted logs, no remote HTML inside privileged webviews, SBOM generation and explicit risk labels for unofficial connectors.