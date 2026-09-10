# Windows runtime acceptance

Compilation and NSIS packaging are not proof of working messaging or WSL provisioning.

## Automated layers

- `pnpm test`: timeline filtering, reactions, replies, read receipts and bounded history snapshots.
- `cargo test -p unibox-core --locked`: bootstrap protocol, output decoding and remote URL policy.
- `scripts/test-bootstrap.ps1`: mocked lifecycle behavior under Windows PowerShell 5.1 and PowerShell 7. This never enables WSL or reboots the runner.
- `scripts/test-windows-runtime.ps1`: actual systemd, PostgreSQL, Synapse, session authentication and listener configuration checks on a dedicated Windows machine.

## Dedicated Windows machine

Use Windows 11 x64 with virtualization and enough disk space. Take a clean VM snapshot before testing. The runtime occupies `UniboxRuntime` and application data belongs to the Windows account running the test. Do not use a machine containing real messaging sessions.

Run `./scripts/test-windows-runtime.ps1 -Prepare` in Windows PowerShell. If it returns 3010, restart Windows and repeat. The script does not restart the machine. Inspect `runtime-e2e.json`; this report includes outcomes only, never tokens or service output.

The manual GitHub Actions workflow uses the labels `self-hosted`, `Windows`, `X64`, `unibox-e2e`. The runner must run under the same Windows user that owns the distribution. Interactive Windows elevation may require running the preparation step locally first. Do not attach untrusted pull request workflows to this runner.

## Required interactive checks

1. Install the CI NSIS package on a clean machine; start setup with WSL disabled.
2. Approve elevation. Confirm a friendly restart screen, with no red PowerShell stack trace.
3. Choose Restart later. Close and reopen Unibox without rebooting; it must still request restart without enabling WSL again.
4. Save work, restart Windows and reopen Unibox. Setup must continue automatically.
5. Connect WhatsApp and Telegram using test accounts. Test inbound/outbound text, files, replies, edits, reactions, read status and supported multiple accounts.
6. Restart Unibox and Windows. Verify history and account sessions remain available.
7. Repeat using a Turkish Windows locale and a username containing spaces and non-ASCII characters.
8. Test canceled elevation, offline setup, interrupted downloads, malformed local session data and connector crashes. Existing session data must not be discarded.
9. Test notifications and attachment downloads in the installed Windows application.

Record the application commit, installer checksum, Windows build, WSL version and outcomes. A signed application-update test and database-aware backup/restore test remain separate release requirements.
