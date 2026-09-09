# CI status

The integration branch is required to pass the Windows desktop pipeline before being merged into the default branch.

The pipeline checks connector metadata, runtime script syntax, TypeScript, the production frontend, the Rust workspace, Rust tests and a release-mode Tauri Windows executable build. Successful builds publish a short-lived `unibox-windows-smoke` artifact for manual smoke testing.

A CI-built executable proves that the application compiles and links on the GitHub Windows toolchain. It does not replace clean-machine testing of the managed WSL2 runtime on physical or normally virtualized Windows hardware.
