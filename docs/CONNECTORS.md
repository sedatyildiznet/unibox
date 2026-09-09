# Connector Model

Target catalog: WhatsApp, Telegram, Signal, Discord, Instagram, Facebook Messenger, Google Messages, Google Chat, Slack, X/Twitter DM, Bluesky, LinkedIn, Google Voice, Zulip, IRC and iMessage where the host platform/upstream implementation permits it.

Connectors are managed plugins. They declare authentication methods and supported message capabilities. Unsupported actions are hidden or disabled in the UI.

A connector becomes Stable only after restart recovery, multi-account behavior, media, normal message operations, migration, update, rollback and disconnection recovery have been tested.

Some networks restrict unofficial third-party clients. Unibox must disclose that clearly and never present an unofficial connector as an official integration.