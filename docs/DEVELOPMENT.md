# Development

Use Node.js 22+, pnpm 9 and the stable Rust toolchain.

Common checks:

```bash
pnpm install
pnpm check
pnpm build
cargo check --workspace
cargo test --workspace
```

On Windows, a release-mode application smoke build can be produced with:

```bash
pnpm tauri build --no-bundle
```

Connector registry changes should also pass:

```bash
python scripts/verify-registry.py
```

Runtime PowerShell and shell scripts are syntax-checked in CI before merge.
