# Architecture

Unibox behaves like a normal desktop messenger while internally managing a local Matrix stack. End users should never need to edit YAML, interact with bridge bots, expose a server, configure Docker, or understand Matrix identifiers.

## Components

1. **Desktop UI** — Tauri 2 + React + TypeScript.
2. **uniboxd** — Rust control daemon bound to loopback/private IPC.
3. **Matrix layer** — Synapse as the compatibility-first homeserver.
4. **Database** — PostgreSQL, with isolated databases per application/connector.
5. **Connector layer** — mautrix bridges behind adapters.
6. **Runtime** — Windows targets a managed WSL2 distribution; native packaging can be evaluated per platform later.

## Data flow

```text
Remote service <-> mautrix connector <-> Synapse <-> matrix-js-sdk <-> Unibox UI
                                      ^
                                      |
                                  PostgreSQL
```

## Isolation

No runtime service binds to a public interface by default. Public registration and Matrix federation are disabled. A bridge is not an account: each connector can expose multiple remote logins, and connectors that cannot safely multiplex can be isolated into separate instances behind the same UI abstraction.

## Connector adapter contract

Adapters implement install, configure, start, stop, restart, health, login, logout, list-logins, reconnect, update, rollback and diagnostics. Service-specific behavior must not leak into generic UI logic.