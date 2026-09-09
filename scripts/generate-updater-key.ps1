$ErrorActionPreference = 'Stop'
Write-Host 'Generate this key on a trusted machine. Never commit the private key.'
pnpm --dir apps/desktop tauri signer generate -w $HOME/.tauri/unibox.key
Write-Host 'Store the private key/password in GitHub Actions secrets and copy only the public key into tauri.conf.json.'
