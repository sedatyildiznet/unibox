# Implementation and acceptance status

The current work improves the engineering build; it is not a production-complete release.

## Implemented in code

- Durable bootstrap states, restart-now/later UI and automatic setup continuation on reopening.
- Background execution of native runtime and connector operations.
- Locale-independent running-distribution detection, staged rootfs verification and setup locking.
- Existing homeserver/appservice configuration preservation during reprovisioning.
- Durable initial local-account password and atomic session-file writing for interrupted first setup.
- DNS-pinned connector HTTP requests, private/special address filtering and cross-origin secret restrictions.
- Text, files, replies, edits, deletion confirmation, reaction counts, typing and read status in the inbox.
- Bounded history expansion, event-driven refresh, notification opt-in and per-room notification mute.
- Python update environments keep their original filesystem paths to preserve interpreter references.

- Version-matched backups with PostgreSQL dumps, validated restores and pre-restore recovery snapshots.
- System tray, optional close-to-tray, Windows startup and start-minimized preferences.
- Allowlisted diagnostic report export without raw logs or session material.

## Verification boundaries

Unit tests and build checks do not establish external-provider compatibility. Windows runtime acceptance is described in `WINDOWS_ACCEPTANCE.md`. No provider account, real Windows/WSL installation or signed application update has been accepted by the automated unit suite.

## Still required before a stable release

- Real Windows acceptance and live WhatsApp/Telegram multi-account messaging tests.
- Provider-specific capability discovery and account rename/reconnect/logout management.
- Full legacy Discord/Google Chat login acceptance and per-provider maturity review.
- Database-aware connector rollback; current binary/venv rollback cannot reverse schema migrations.
- Windows acceptance of backup/restore, durable recovery after forced machine shutdown, optional encryption and desktop-preference backup.
- Installed-Windows acceptance of tray/startup behavior, comprehensive diagnostics and cache controls.
- Global indexed message search, media preview, voice notes, threads, stickers and polls.
- Signed updater keys/public key configuration and an actual signed upgrade acceptance test.
- Installer/uninstaller data-removal options and clean-machine release acceptance.

Do not label a connector Stable based only on repository existence, downloadable binaries or passing compilation.
