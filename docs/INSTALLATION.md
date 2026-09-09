# Installation Design

## End-user Windows goal

1. Download `Unibox-Setup.exe` from GitHub Releases.
2. Run the signed installer.
3. Unibox verifies/enables required Windows virtualization/WSL components if needed.
4. A managed `UniboxRuntime` WSL2 distribution is imported automatically.
5. PostgreSQL, Synapse and connector services initialize privately.
6. Unibox opens and the user selects **Add service**.

Users should not need Docker Desktop, terminal commands, YAML editing or a Matrix account.

The current `installer/windows-install.ps1` is a development scaffold, not a production installer.