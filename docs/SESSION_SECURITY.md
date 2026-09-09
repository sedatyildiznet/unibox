# Session security

Connector session material is stored locally because persistent sign-in requires it. Session tokens and cookies must be treated as secrets: they are not written to ordinary diagnostics, not included in telemetry and not committed to source control. Account logout should remove connector-side session material according to each adapter's supported cleanup path.
