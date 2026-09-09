import { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  Archive,
  AtSign,
  Download,
  Inbox,
  LoaderCircle,
  Plus,
  Search,
  Send,
  Settings,
  Star,
  X,
} from 'lucide-react';
import { QRCodeSVG } from 'qrcode.react';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import {
  backend,
  type ConnectorDefinition,
  type LoginField,
  type LoginFlow,
  type LoginStep,
  type RuntimeStatus,
} from '../lib/backend';
import { startInbox, type InboxController, type InboxRoom } from '../lib/matrix';

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isProvisioningConnector(connector: ConnectorDefinition): boolean {
  return connector.adapter === 'bridgev2' || connector.adapter === 'source-go';
}

export function App() {
  const [registry, setRegistry] = useState<ConnectorDefinition[]>([]);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const [runtimeError, setRuntimeError] = useState('');
  const [rooms, setRooms] = useState<InboxRoom[]>([]);
  const [inbox, setInbox] = useState<InboxController | null>(null);
  const [selectedRoomId, setSelectedRoomId] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [composer, setComposer] = useState('');
  const [showServices, setShowServices] = useState(false);
  const [activeConnector, setActiveConnector] = useState<ConnectorDefinition | null>(null);
  const [connectorBusy, setConnectorBusy] = useState(false);
  const [connectorError, setConnectorError] = useState('');
  const [flows, setFlows] = useState<LoginFlow[]>([]);
  const [loginStep, setLoginStep] = useState<LoginStep | null>(null);
  const [loginValues, setLoginValues] = useState<Record<string, string>>({});
  const [updating, setUpdating] = useState(false);
  const [updateMessage, setUpdateMessage] = useState('Check for updates');

  const selectedRoom = rooms.find(room => room.id === selectedRoomId) ?? rooms[0];
  const filteredRooms = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return rooms;
    return rooms.filter(room =>
      `${room.name} ${room.service} ${room.preview}`.toLowerCase().includes(needle),
    );
  }, [rooms, query]);

  async function refreshRuntime(): Promise<RuntimeStatus> {
    const next = await backend.status();
    setRuntime(next);
    return next;
  }

  useEffect(() => {
    void backend.registry().then(setRegistry).catch(error => setRuntimeError(errorText(error)));
    void refreshRuntime().catch(error => setRuntimeError(errorText(error)));
  }, []);

  useEffect(() => {
    if (!runtime?.synapse_ready || !runtime.matrix_session_ready) return;
    let disposed = false;
    let controller: InboxController | null = null;

    void backend
      .matrixSession()
      .then(session => startInbox(session, nextRooms => {
        if (!disposed) setRooms(nextRooms);
      }))
      .then(next => {
        if (disposed) {
          next.stop();
          return;
        }
        controller = next;
        setInbox(next);
      })
      .catch(error => {
        if (!disposed) setRuntimeError(errorText(error));
      });

    return () => {
      disposed = true;
      controller?.stop();
      setInbox(null);
    };
  }, [runtime?.synapse_ready, runtime?.matrix_session_ready]);

  useEffect(() => {
    if (!selectedRoomId && rooms[0]) setSelectedRoomId(rooms[0].id);
    if (selectedRoomId && rooms.length && !rooms.some(room => room.id === selectedRoomId)) {
      setSelectedRoomId(rooms[0].id);
    }
  }, [rooms, selectedRoomId]);

  useEffect(() => {
    if (!activeConnector || !loginStep) return;

    if (loginStep.type === 'display_and_wait') {
      const connector = activeConnector;
      const step = loginStep;
      const txn = step.txn_id ? `?txn_id=${encodeURIComponent(step.txn_id)}` : '';
      void backend
        .provision<LoginStep>(
          connector.id,
          'POST',
          `/v3/login/step/${encodeURIComponent(step.login_id)}/${encodeURIComponent(step.step_id)}/display_and_wait${txn}`,
          {},
        )
        .then(setLoginStep)
        .catch(error => setConnectorError(errorText(error)));
      return;
    }

    if (loginStep.type === 'client_http' && loginStep.client_http) {
      const connector = activeConnector;
      const step = loginStep;
      void backend
        .clientHttp(step.client_http)
        .then(response =>
          backend.provision<LoginStep>(
            connector.id,
            'POST',
            `/v3/login/client_http/${encodeURIComponent(step.login_id)}/${encodeURIComponent(step.txn_id ?? '')}/${encodeURIComponent(step.client_http!.request_id)}`,
            response,
          ),
        )
        .then(setLoginStep)
        .catch(error => setConnectorError(errorText(error)));
    }
  }, [activeConnector, loginStep]);

  async function installRuntime(): Promise<void> {
    setRuntimeBusy(true);
    setRuntimeError('');
    try {
      await backend.bootstrap();
      await backend.startRuntime();
      await refreshRuntime();
    } catch (error) {
      setRuntimeError(errorText(error));
    } finally {
      setRuntimeBusy(false);
    }
  }

  async function runUpdate(): Promise<void> {
    setUpdating(true);
    setUpdateMessage('Checking…');
    try {
      const update = await check();
      if (!update) {
        setUpdateMessage('You are up to date');
        return;
      }
      setUpdateMessage(`Installing ${update.version}…`);
      await update.downloadAndInstall();
      await relaunch();
    } catch (error) {
      console.error(error);
      setUpdateMessage('Update check failed');
    } finally {
      setUpdating(false);
    }
  }

  async function connectService(connector: ConnectorDefinition): Promise<void> {
    setActiveConnector(connector);
    setShowServices(false);
    setConnectorBusy(true);
    setConnectorError('');
    setFlows([]);
    setLoginStep(null);
    setLoginValues({});

    try {
      let status = await backend.connectorStatus(connector.id);
      if (!status.installed) {
        await backend.installConnector(connector.id);
        status = await backend.connectorStatus(connector.id);
      }
      if (!status.running) await backend.startConnector(connector.id);

      if (isProvisioningConnector(connector)) {
        const response = await backend.provision<{ flows: LoginFlow[] }>(
          connector.id,
          'GET',
          '/v3/login/flows',
        );
        const nextFlows = response.flows ?? [];
        setFlows(nextFlows);
        if (nextFlows.length === 1) await beginFlow(connector, nextFlows[0]);
      } else {
        await beginLegacyLogin(connector);
      }
    } catch (error) {
      setConnectorError(errorText(error));
    } finally {
      setConnectorBusy(false);
    }
  }

  async function beginFlow(connector: ConnectorDefinition, flow: LoginFlow): Promise<void> {
    setConnectorBusy(true);
    setConnectorError('');
    try {
      const step = await backend.provision<LoginStep>(
        connector.id,
        'POST',
        `/v3/login/start/${encodeURIComponent(flow.id)}?client_http=1`,
        {},
      );
      setLoginValues({});
      setLoginStep(step);
    } catch (error) {
      setConnectorError(errorText(error));
    } finally {
      setConnectorBusy(false);
    }
  }

  async function beginLegacyLogin(connector: ConnectorDefinition): Promise<void> {
    if (!inbox) throw new Error('Local Matrix client is not ready yet.');
    const bot = `@${connector.bot_localpart || `${connector.id}bot`}:unibox.local`;
    const created = await inbox.client.createRoom({
      is_direct: true,
      invite: [bot],
      name: `${connector.name} · Unibox setup`,
    });
    await inbox.client.sendTextMessage(created.room_id, connector.legacy_login || 'login');
    setSelectedRoomId(created.room_id);
    setActiveConnector(null);
  }

  async function submitLoginStep(): Promise<void> {
    if (!activeConnector || !loginStep) return;
    const type = loginStep.type;
    if (type !== 'user_input' && type !== 'cookies') return;

    setConnectorBusy(true);
    setConnectorError('');
    try {
      const txn = loginStep.txn_id ? `?txn_id=${encodeURIComponent(loginStep.txn_id)}` : '';
      const next = await backend.provision<LoginStep>(
        activeConnector.id,
        'POST',
        `/v3/login/step/${encodeURIComponent(loginStep.login_id)}/${encodeURIComponent(loginStep.step_id)}/${type}${txn}`,
        loginValues,
      );
      setLoginValues({});
      setLoginStep(next);
    } catch (error) {
      setConnectorError(errorText(error));
    } finally {
      setConnectorBusy(false);
    }
  }

  async function sendMessage(): Promise<void> {
    if (!inbox || !selectedRoom || !composer.trim()) return;
    const text = composer;
    setComposer('');
    try {
      await inbox.sendText(selectedRoom.id, text);
    } catch (error) {
      setRuntimeError(errorText(error));
      setComposer(text);
    }
  }

  if (!runtime?.synapse_ready || !runtime.matrix_session_ready) {
    return (
      <div className="onboarding">
        <div className="onboardingCard">
          <div className="brandMark large">U</div>
          <h1>Unibox</h1>
          <p className="tagline">All your chats. One box.</p>
          <h2>Set up your private local engine</h2>
          <p>
            Unibox stores its Matrix database, connector sessions, media and settings on this PC
            inside an isolated <strong>UniboxRuntime</strong> WSL distribution.
          </p>
          <div className="privacyCard">
            <strong>No Unibox cloud account.</strong>
            <span>Your chat database and service sessions are not uploaded to an Unibox server.</span>
          </div>
          {runtimeError && <ErrorBox text={runtimeError} />}
          <button className="primaryButton" disabled={runtimeBusy} onClick={() => void installRuntime()}>
            {runtimeBusy ? <LoaderCircle className="spin" size={18} /> : <Download size={18} />}
            {runtimeBusy ? 'Installing local engine…' : 'Install local engine'}
          </button>
          <small>
            Windows 10/11 with WSL2 is required. The official Ubuntu rootfs is checksum-verified
            before import.
          </small>
        </div>
      </div>
    );
  }

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brandMark">U</div>
          <div><strong>Unibox</strong><span>All your chats. One box.</span></div>
        </div>
        <nav>
          <button className="active"><Inbox size={17} />All Chats <b>{rooms.length}</b></button>
          <button><Star size={17} />Unread</button>
          <button><AtSign size={17} />Mentions</button>
          <button><Archive size={17} />Archive</button>
        </nav>
        <div className="sectionTitle">Connected networks</div>
        {[...new Set(rooms.map(room => room.service))].slice(0, 8).map(service => (
          <button className="service" key={service}>
            <span className="serviceDot" />{service}<i />
          </button>
        ))}
        <button className="add" onClick={() => setShowServices(true)}><Plus size={17} />Add service</button>
        <div className="sidebarBottom">
          <button disabled={updating} onClick={() => void runUpdate()}><Download size={16} />{updateMessage}</button>
          <button><Settings size={16} />Settings</button>
        </div>
      </aside>

      <section className="listPane">
        <div className="search">
          <Search size={17} />
          <input value={query} onChange={event => setQuery(event.target.value)} aria-label="Search" placeholder="Search conversations…" />
          <kbd>Ctrl K</kbd>
        </div>
        <div className="filters"><span className="selected">All</span><span>Direct</span><span>Groups</span><span>Unread</span><span>Favorites</span></div>
        <div className="chatList">
          {filteredRooms.length === 0 && <div className="emptyList">No conversations yet.<br />Add a service to get started.</div>}
          {filteredRooms.map(room => (
            <button
              key={room.id}
              onClick={() => setSelectedRoomId(room.id)}
              className={selectedRoom?.id === room.id ? 'chat activeChat' : 'chat'}
            >
              <div className="avatar">{room.name[0]?.toUpperCase() || '?'}</div>
              <div className="chatMeta"><strong>{room.name}</strong><small>{room.service}</small><span>{room.preview}</span></div>
            </button>
          ))}
        </div>
      </section>

      <main className="conversation">
        {selectedRoom ? (
          <>
            <header>
              <div><h2>{selectedRoom.name}</h2><span>{selectedRoom.service}</span></div>
              <div className="headerActions"><button><Search size={18} /></button><button><Settings size={18} /></button></div>
            </header>
            <div className="messages">
              {selectedRoom.messages.map(message => (
                <div key={message.id} className={message.mine ? 'bubble mine' : 'bubble'}>
                  <b>{message.mine ? 'You' : message.sender}</b>
                  <p>{message.body}</p>
                  <small>{new Date(message.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</small>
                </div>
              ))}
            </div>
            <div className="composer">
              <button aria-label="Add attachment"><Plus size={18} /></button>
              <input
                value={composer}
                onChange={event => setComposer(event.target.value)}
                onKeyDown={event => {
                  if (event.key === 'Enter' && !event.shiftKey) void sendMessage();
                }}
                placeholder={`Message ${selectedRoom.name}…`}
              />
              <button className="send" onClick={() => void sendMessage()} aria-label="Send"><Send size={18} /></button>
            </div>
          </>
        ) : (
          <div className="conversationEmpty">
            <div className="brandMark large">U</div>
            <h2>Your inbox is ready</h2>
            <p>Add a service to bring your conversations into Unibox.</p>
            <button className="primaryButton" onClick={() => setShowServices(true)}><Plus size={18} />Add service</button>
          </div>
        )}
      </main>

      {showServices && (
        <div className="modalBackdrop" onMouseDown={() => setShowServices(false)}>
          <div className="modal servicePicker" onMouseDown={event => event.stopPropagation()}>
            <div className="modalHeader">
              <div><h2>Add a service</h2><p>Connect another account to your local Unibox.</p></div>
              <button onClick={() => setShowServices(false)}><X size={20} /></button>
            </div>
            <div className="serviceGrid">
              {registry.map(connector => (
                <button key={connector.id} className="serviceCard" onClick={() => void connectService(connector)}>
                  <div className="serviceCardIcon">{connector.name[0]}</div>
                  <div>
                    <strong>{connector.name}</strong>
                    <span>{connector.description}</span>
                    <small>{connector.maturity}{connector.multi_account ? ' · Multi-account' : ''}</small>
                  </div>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {activeConnector && (
        <div className="modalBackdrop">
          <div className="modal loginModal">
            <div className="modalHeader">
              <div><h2>Connect {activeConnector.name}</h2><p>{activeConnector.description}</p></div>
              <button onClick={() => setActiveConnector(null)}><X size={20} /></button>
            </div>
            {activeConnector.risk_notice && <WarningBox text={activeConnector.risk_notice} />}
            {connectorError && <ErrorBox text={connectorError} />}
            {connectorBusy && <div className="loadingRow"><LoaderCircle className="spin" size={20} />Preparing connector…</div>}
            {!connectorBusy && !loginStep && flows.length > 1 && (
              <div className="flowList">
                {flows.map(flow => (
                  <button key={flow.id} onClick={() => void beginFlow(activeConnector, flow)}>
                    <strong>{flow.name}</strong><span>{flow.description}</span>
                  </button>
                ))}
              </div>
            )}
            {!connectorBusy && !loginStep && flows.length === 0 && !connectorError && (
              <div className="loadingRow"><LoaderCircle className="spin" size={20} />Starting connector…</div>
            )}
            {loginStep && (
              <LoginStepView
                step={loginStep}
                values={loginValues}
                setValues={setLoginValues}
                submit={() => void submitLoginStep()}
                busy={connectorBusy}
              />
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ErrorBox({ text }: { text: string }) {
  return <div className="errorBox"><AlertTriangle size={18} /><span>{text}</span></div>;
}

function WarningBox({ text }: { text: string }) {
  return <div className="warningBox"><AlertTriangle size={18} /><span>{text}</span></div>;
}

function LoginStepView({
  step,
  values,
  setValues,
  submit,
  busy,
}: {
  step: LoginStep;
  values: Record<string, string>;
  setValues: (next: Record<string, string>) => void;
  submit: () => void;
  busy: boolean;
}) {
  if (step.type === 'complete') {
    return (
      <div className="loginComplete">
        <div className="successMark">✓</div>
        <h3>Account connected</h3>
        <p>Your conversations will appear as the bridge finishes syncing.</p>
      </div>
    );
  }

  if (step.type === 'display_and_wait') {
    const display = step.display_and_wait;
    return (
      <div className="qrStep">
        <p>{step.instructions}</p>
        {display?.type === 'qr' && display.data && <QRCodeSVG value={display.data} size={230} marginSize={2} />}
        {(display?.type === 'code' || display?.type === 'emoji') && <div className="pairingCode">{display.data}</div>}
        <span>Waiting for confirmation…</span>
      </div>
    );
  }

  if (step.type === 'client_http') {
    return <div className="loadingRow"><LoaderCircle className="spin" size={20} />Completing secure sign-in request…</div>;
  }

  if (step.type === 'webauthn') {
    return <WarningBox text="This connector requested a WebAuthn security-key step. Hardware-key login is not available in this desktop build yet; choose another login flow if offered." />;
  }

  const fields: LoginField[] = step.type === 'user_input'
    ? (step.user_input?.fields ?? [])
    : (step.cookies?.fields ?? []).map(field => ({
        id: field.id,
        name: field.id,
        type: 'password',
        description: `Cookie value${field.required ? ' (required)' : ''}`,
      }));

  return (
    <div className="loginForm">
      {step.instructions && <p>{step.instructions}</p>}
      {step.type === 'cookies' && step.cookies?.url && (
        <WarningBox text={`This connector requires browser-session values from ${new URL(step.cookies.url).hostname}. Values are sent only to the local connector.`} />
      )}
      {fields.map(field => (
        <label key={field.id}>
          <span>{field.name}</span>
          {field.description && <small>{field.description}</small>}
          {field.options ? (
            <select
              value={values[field.id] ?? field.default_value ?? ''}
              onChange={event => setValues({ ...values, [field.id]: event.target.value })}
            >
              {field.options.map((option: string) => <option key={option}>{option}</option>)}
            </select>
          ) : (
            <input
              type={field.type === 'password' || field.type === '2fa_code' || step.type === 'cookies' ? 'password' : 'text'}
              value={values[field.id] ?? field.default_value ?? ''}
              onChange={event => setValues({ ...values, [field.id]: event.target.value })}
            />
          )}
        </label>
      ))}
      <button className="primaryButton" disabled={busy} onClick={submit}>
        {busy ? <LoaderCircle className="spin" size={18} /> : null}Continue
      </button>
    </div>
  );
}
