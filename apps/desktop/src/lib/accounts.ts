export type ConnectorLoginsResponse = {
  login_ids?: unknown;
};

export function parseConnectorLoginIds(response: unknown): string[] {
  if (!response || typeof response !== 'object') return [];
  const loginIds = (response as ConnectorLoginsResponse).login_ids;
  if (!Array.isArray(loginIds)) return [];

  return [...new Set(
    loginIds
      .filter((value): value is string => typeof value === 'string')
      .map(value => value.trim())
      .filter(Boolean),
  )];
}

function requiredSegment(value: string, name: string): string {
  const normalized = value.trim();
  if (!normalized) throw new Error(`${name} must not be empty`);
  return encodeURIComponent(normalized);
}

export function buildLoginStartPath(flowId: string, existingLoginId?: string | null): string {
  const query = new URLSearchParams({ client_http: '1' });
  if (existingLoginId?.trim()) query.set('login_id', existingLoginId.trim());
  return `/v3/login/start/${requiredSegment(flowId, 'flow ID')}?${query.toString()}`;
}

export function buildLogoutPath(loginId: string): string {
  return `/v3/logout/${requiredSegment(loginId, 'login ID')}`;
}

export function buildCancelLoginPath(loginProcessId: string): string {
  return `/v3/login/cancel/${requiredSegment(loginProcessId, 'login process ID')}`;
}
