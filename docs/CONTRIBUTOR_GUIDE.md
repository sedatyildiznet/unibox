# Contributor guide

Keep user-facing behavior local-first and avoid leaking Matrix implementation details into ordinary UI. New connectors must declare their adapter type and capabilities. Changes affecting authentication, updates, storage or networking should include corresponding security/privacy documentation and CI coverage.
