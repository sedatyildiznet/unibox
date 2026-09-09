# Architecture decisions

- Tauri 2 is the desktop shell.
- React and TypeScript implement the renderer UI.
- Rust owns the local runtime manager and privileged orchestration boundary.
- Matrix/Synapse provides the internal normalized messaging model.
- PostgreSQL backs Synapse and connector state inside the managed runtime.
- Windows uses a private WSL2 runtime to avoid exposing Docker/Matrix setup to users.
- Connectors are adapter-driven because mautrix projects do not all share one control API.
- The default managed runtime does not use Matrix federation.
- Unibox does not require a central Unibox account or hosted message database.
