import { invoke } from '@tauri-apps/api/core';

export type RuntimeStatus = {
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

export type ConnectorRequirementField = {
  id: string;
  name: string;
  type: 'text' | 'password';
  description?: string;
};

export type ConnectorRequirements = {
  required: boolean;
  fields: ConnectorRequirementField[];
  help?: string;
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
  webauthn?: {
    url?: string;
    publicKey?: Record<string, unknown>;
  };
  client_http?: {
    request_id: string;
    method: string;
    url: string;
    headers?: Record<string, string[]>;
    body?: string;
  };
  complete?: { user_login_id: string };
};

export const backend = {
  registry: () => invoke<ConnectorDefinition[]>('connector_registry'),
  status: () => invoke<RuntimeStatus>('runtime_status'),
  bootstrap: () => invoke<string>('bootstrap_runtime'),
  startRuntime: () => invoke<string>('start_runtime'),
  stopRuntime: () => invoke<string>('stop_runtime'),
  matrixSession: () => invoke<MatrixSession>('matrix_session'),
  connectorStatus: (id: string) => invoke<ConnectorStatus>('connector_status', { id }),
  connectorRequirements: (id: string) => invoke<ConnectorRequirements>('connector_requirements', { id }),
  configureConnector: (id: string, settings: Record<string, string>) =>
    invoke<string>('connector_configure', { id, settings }),
  installConnector: (id: string) => invoke<string>('connector_install', { id }),
  startConnector: (id: string) => invoke<string>('connector_start', { id }),
  stopConnector: (id: string) => invoke<string>('connector_stop', { id }),
  updateConnector: (id: string) => invoke<string>('connector_update', { id }),
  provision: <T>(id: string, method: string, path: string, body?: unknown) =>
    invoke<T>('connector_provision', { id, method, path, body: body ?? null }),
  clientHttp: (request: unknown) => invoke<unknown>('connector_client_http', { request }),
};
