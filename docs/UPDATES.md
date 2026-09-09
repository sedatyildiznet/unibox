# Update System

Unibox has three independently versioned tracks: desktop application, managed runtime and connectors.

## Desktop

The Tauri updater checks signed metadata published with GitHub Releases. Tauri requires updater signatures; the configured public key validates artifacts before installation. The private key must never be committed.

## Connectors

```text
download -> verify source/hash -> stop -> backup -> replace -> health check -> commit
                                                          | failure
                                                          v
                                                       rollback
```

## Runtime

Runtime updates must be transactional. Destructive database migrations require a tested pre-upgrade backup and rollback/recovery path.