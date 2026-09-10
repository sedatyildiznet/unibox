# Backup and restore

Use **Settings → Create backup** or **Restore backup** while the local engine is healthy. Backups use the `.uniboxbackup` extension.

Backups contain private messages, media, local account sessions, connector state, engine configuration and PostgreSQL custom-format dumps. They are **not encrypted**; store them privately. Restore only backups you created and trust. File hashes detect corruption, not a malicious author who can replace the manifest.

The implementation pauses active messaging services, copies application data and runs `pg_dump` for each application database. It never copies PostgreSQL's active storage directory. Services resume after the snapshot is created. Installer files, executable packages and Windows desktop preferences are not included.

Restore requires matching Synapse, PostgreSQL major version and connector executable/Python package fingerprints. Install the matching versions before restoring an older backup. This avoids running old state against incompatible migrations.

Before changing live data, restore checks archive structure, per-file hashes, expected data roots, database lists, version fingerprints and database archive readability. It then snapshots the current state, restores each database in a transaction and restores application files. Health-check failure triggers recovery from the independent pre-restore snapshot. If automatic recovery itself fails, a private journal and snapshot are retained under `/var/lib/unibox-maintenance/` inside the local engine. Do not delete it; recover the engine before reconnecting accounts.

The application prevents normal window/tray exit while maintenance is active. Leave Windows running and allow sufficient free disk space for incoming data and the recovery snapshot. A durable journal records the pre-restore snapshot before data changes. On the next normal startup, a pending recovery is completed before the inbox connects. Forced-shutdown recovery still needs acceptance testing on real Windows/WSL.

Automated tests cover archive rejection, version mismatch, integrity validation, real PostgreSQL dump/restore and recovery after a simulated health failure. Clean Windows/WSL acceptance remains required before a stable release. Password encryption, cross-version migration and full desktop-preference backup remain outstanding.
