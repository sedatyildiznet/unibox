# API boundaries

The renderer communicates with the local runtime only through explicit Tauri commands. The renderer does not directly spawn WSL processes, edit Synapse configuration or execute downloaded connector binaries.

The runtime manager translates stable Unibox operations into WSL/Synapse/connector-specific actions. Provisioning responses are normalized before they reach UI flows. This boundary reduces the amount of privileged orchestration exposed to renderer code.
