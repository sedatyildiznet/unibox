import { invoke } from '@tauri-apps/api/core';
import {
  buildCancelLoginPath,
  buildLoginStartPath,
  buildLogoutPath,
  parseConnectorLoginIds,
} from './accounts';

export type BootstrapResult = {
  state: 'INSTALLING_WSL' | 'REBOOT_REQUIRED' | 'RUNTIME_INSTALLING' | 'RUNTIME_READY' | 'ERROR';
  message: string;
};

export type RuntimeStatus = {
  bootstrap?: BootstrapResult | null;
  platform: string;
  wsl_available: boolean;
  distro_installed: boolean;
  distro_running: boolean;
  synapse_ready: boolean;
  matrix_session_ready: boolean;
  data_root: string;
};

export type MatrixSession = {
  homeserver: string;
  user_id: string;
  access_token: string;
  device_id?: string | null;
};

export type ConnectorDefinition = {
  id: string;
  name: string;
  category: string;
  repository: string;
  binary: string;
  asset_linux_amd64: string;
  port: number;
  maturity: string;
  adapter: 'bridgev2' | 'legacy-go' | 'python-legacy' | string;
  description: string;
  multi_account: boolean;
  risk_notice?: string | null;
  bot_localpart?: string | null;
  legacy_login?: string | null;
};

export type ConnectorStatus = {
  id: string;
  installed: boolean;
  running: boolean;
  provisioning_url?: string | null;
};

export type LoginFlow = { id: string; name: string; description: string };
export type LoginField = {
  type: string;
  id: string;
  name: string;
  description?: string;
  default_value?: string;
  pattern?: string;
  options?: string[];
};
export type LoginStep = {
  login_id: string;
  step_id: string;
  txn_id?: string;
  type: 'user_input' | 'display_and_wait' | 'cookies' | 'webauthn' | 'client_http' | 'complete';
  instructions?: string;
  user_input?: { fields: LoginField[] };
  display_and_wait?: { type: 'nothing' | 'emoji' | 'qr' | 'code'; data?: string; image_url?: string };
  cookies?: { url: string; fields: Array<{ id: string; required: boolean }> };
  client_http?: {
    request_id: string;
    method: string;
    url: string;
    headers?: Record<string, string[]>;
    body?: string;
  };
  complete?: { user_login_id: string };
};

export type DesktopPreferences = { close_to_tray: boolean; run_at_startup: boolean; start_minimized: boolean };

function provision<T>(id: string, method: string, path: string, body?: unknown): Promise<T> {
  return invoke<T>('connector_provision', { id, method, path, body: body ?? null });
}

export const backend = {
  desktopPreferences: () => invoke<DesktopPreferences>('desktop_preferences'),
  saveDesktopPreferences: (preferences: DesktopPreferences) => invoke<void>('save_desktop_preferences', { preferences }),
  maintenance: (operation: 'backup' | 'restore', destination: string) => invoke<string>('runtime_maintenance', { operation, destination }),
  exportDiagnostics: (destination: string) => invoke<void>('export_diagnostics', { destination }),
  registry: () => invoke<ConnectorDefinition[]>('connector_registry'),
  status: () => invoke<RuntimeStatus>('runtime_status'),
  bootstrap: () => invoke<BootstrapResult>('bootstrap_runtime'),
  restartWindows: () => invoke<void>('restart_windows'),
  startRuntime: () => invoke<string>('start_runtime'),
  stopRuntime: () => invoke<string>('stop_runtime'),
  matrixSession: () => invoke<MatrixSession>('matrix_session'),
  connectorStatus: (id: string) => invoke<ConnectorStatus>('connector_status', { id }),
  installConnector: (id: string) => invoke<string>('connector_install', { id }),
  startConnector: (id: string) => invoke<string>('connector_start', { id }),
  stopConnector: (id: string) => invoke<string>('connector_stop', { id }),
  updateConnector: (id: string) => invoke<string>('connector_update', { id }),
  provision: <T>(id: string, method: string, path: string, body?: unknown) => provision<T>(id, method, path, body),
  connectorLoginFlows: async (id: string) => {
    const response = await provision<{ flows?: LoginFlow[] }>(id, 'GET', '/v3/login/flows');
    return Array.isArray(response.flows) ? response.flows : [];
  },
  connectorLogins: async (id: string) => {
    const response = await provision<unknown>(id, 'GET', '/v3/logins');
    return parseConnectorLoginIds(response);
  },
  startConnectorLogin: (id: string, flowId: string, existingLoginId?: string | null) =>
    provision<LoginStep>(id, 'POST', buildLoginStartPath(flowId, existingLoginId), {}),
  logoutConnectorLogin: (id: string, loginId: string) =>
    provision<Record<string, never>>(id, 'POST', buildLogoutPath(loginId), {}),
  cancelConnectorLogin: (id: string, loginProcessId: string) =>
    provision<Record<string, never>>(id, 'POST', buildCancelLoginPath(loginProcessId), {}),
  clientHttp: (request: unknown) => invoke<unknown>('connector_client_http', { request }),
};
