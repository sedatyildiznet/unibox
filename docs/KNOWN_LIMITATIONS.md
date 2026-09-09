# Known limitations

- Messaging connectors depend on upstream services and may break when those services change protocols or authentication requirements.
- Some connectors use unofficial client protocols and can be subject to service-side restrictions or additional verification.
- A successful Windows CI build verifies compilation and packaging, not the behavior of WSL2/systemd on every end-user machine.
- Voice/video calling is outside the initial desktop messaging scope.
- Connector capability parity is not guaranteed; the UI must adapt to each network's supported features.
- Public production releases require Tauri updater signing configuration before automatic updates can be safely enabled.
