# Build verification

The current integration pipeline has successfully produced a release-mode Windows executable on GitHub Actions after passing connector registry validation, runtime-script syntax checks, TypeScript checks, the frontend production build, Rust compilation and Rust tests.

The CI executable is intended for smoke testing. Production releases additionally require updater signing and clean-machine runtime acceptance testing.
