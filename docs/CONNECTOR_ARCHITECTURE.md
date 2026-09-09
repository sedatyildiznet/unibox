# Connector architecture

Unibox does not assume every mautrix bridge exposes the same control surface.

## BridgeV2 connectors

Modern mautrix bridges are integrated through the BridgeV2 provisioning API. The adapter supports multi-step authentication flows such as user input, QR/code display, cookie capture, WebAuthn and client-side HTTP handoffs.

## Legacy Go connectors

Connectors such as mautrix-discord use their own legacy management/login model. Unibox treats them as a separate adapter type rather than pretending they implement BridgeV2.

## Legacy Python connectors

Connectors such as mautrix-googlechat have a separate Python runtime and legacy provisioning surface. They are installed and controlled through a dedicated adapter path.

## Why adapters matter

The desktop UI talks to a stable Unibox connector interface. Individual adapters translate install, start, stop, update, login, logout, status and health operations into the mechanism supported by each upstream bridge. This keeps upstream protocol or bridge architecture changes from leaking directly into the UI.

## Account model

A connector is not the same as an account. One installed connector can expose multiple remote logins when the upstream bridge supports it. The UI therefore models service, connector installation and remote account as separate concepts.
