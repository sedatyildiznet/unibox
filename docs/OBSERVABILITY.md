# Observability

Runtime health is represented through explicit component status rather than requiring users to read raw logs. Health surfaces should cover WSL, PostgreSQL, Synapse and each connector independently. Logs remain local and diagnostics must redact authentication material before export.
