# Networking

Unibox local infrastructure is not designed to be exposed as a public server. Synapse client access is bound to localhost in the managed runtime. Connectors make outbound network connections to the messaging services users explicitly connect. Updater and connector installation flows make outbound requests to GitHub/upstream release sources. Public inbound access is not required for normal single-user desktop operation.
