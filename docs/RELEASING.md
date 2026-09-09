# Release Process

Before the first public release:

1. Generate a Tauri updater signing keypair on a trusted machine.
2. Put `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in GitHub Actions secrets.
3. Replace `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY` in `tauri.conf.json` with only the public key.
4. Add Windows Authenticode signing for production distribution; updater signatures protect update integrity but do not replace Windows publisher signing/SmartScreen reputation.
5. Update version and changelog, run CI/acceptance tests, create a version tag, verify the draft release and publish only after installation/upgrade tests pass.

Never commit private signing material.