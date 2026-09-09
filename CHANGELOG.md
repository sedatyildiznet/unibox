# Changelog

## Unreleased

- Treat Windows restart requirements as durable setup states, with restart-now/later controls and automatic continuation when reopening Unibox.
- Keep long local-engine and connector operations off the desktop event thread.
- Detect running distributions without parsing localized Windows status words.
- Stage and verify runtime downloads; retain imported runtime data when setup is interrupted.
- Add PowerShell lifecycle and Rust bootstrap protocol regression coverage.
- Accept source-built BridgeV2 connectors through the provisioning adapter.


## 0.1.0 - Foundation

- Tauri 2 / React desktop shell
- Rust `uniboxd` loopback health service
- Local-first architecture and security documentation
- Connector registry model
- GitHub Release updater wiring
- Windows managed-runtime bootstrap scaffold
- CI and draft release workflows
