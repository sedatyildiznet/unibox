import { useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  Archive,
  AtSign,
  Download,
  Inbox,
  LoaderCircle,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Search,
  Send,
  Settings,
  Star,
  Trash2,
  X,
} from 'lucide-react';
import { QRCodeSVG } from 'qrcode.react';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import {
  backend,
  type ConnectorDefinition,
  type ConnectorStatus,
  type ConnectorRequirements,
  type LoginField,
  type LoginFlow,
  type LoginStep,
  type RuntimeStatus,
} from '../lib/backend';
import { startInbox, type InboxController, type InboxRoom } from '../lib/matrix';

type NavMode = 'all' | 'unread' | 'mentions' | 'archive' | 'favorites';
type ListFilter = 'all' | 'direct' | 'groups' | 'unread' | 'favorites';

function errorText(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  const lower = raw.toLowerCase();

  if (lower.includes('bridge provisioning request failed') || lower.includes('connector client http request failed')) {
    return 'The local connector could not complete sign-in. Retry the connection; technical details are kept in the local runtime logs.';
  }
  if (lower.includes('bridge provisioning returned')) {
    return 'The service rejected the current sign-in step. Retry the connection or choose another available login method.';
  }
  if (lower.includes('invalid connector config') || lower.includes('configuration error')) {
    return 'Unibox could not prepare this service configuration. Reinstall the current complete Unibox build and retry.';
  }
  if (lower.includes('api_hash') || lower.includes('telegram application credentials')) {
    return 'This Unibox build is missing its Telegram application credentials. Install a complete release build; you should never need to enter API credentials yourself.';
  }

  return raw;
}

function isProvisioningConnector(connector: ConnectorDefinition): boolean {
  return connector.adapter === 'bridgev2' || connector.adapter === 'source-go';
}

const SERVICE_ICONS: Record<string, string> = {
  whatsapp: new URL('../assets/service-icons/whatsapp.svg', import.meta.url).href,
  telegram: new URL('../assets/service-icons/telegram.svg', import.meta.url).href,
  signal: new URL('../assets/service-icons/signal.svg', import.meta.url).href,
  discord: new URL('../assets/service-icons/discord.svg', import.meta.url).href,
  instagram: new URL('../assets/service-icons/instagram.svg', import.meta.url).href,
  messenger: new URL('../assets/service-icons/messenger.svg', import.meta.url).href,
  gmessages: new URL('../assets/service-icons/gmessages.svg', import.meta.url).href,
  googlechat: new URL('../assets/service-icons/googlechat.svg', import.meta.url).href,
  slack: new URL('../assets/service-icons/slack.svg', import.meta.url).href,
  linkedin: new URL('../assets/service-icons/linkedin.svg', import.meta.url).href,
  gvoice: new URL('../assets/service-icons/gvoice.svg', import.meta.url).href,
  twitter: new URL('../assets/service-icons/twitter.svg', import.meta.url).href,
  bluesky: new URL('../assets/service-icons/bluesky.svg', import.meta.url).href,
  zulip: new URL('../assets/service-icons/zulip.svg', import.meta.url).href,
  irc: new URL('../assets/service-icons/irc.svg', import.meta.url).href,
};

function ServiceIcon({ connector }: { connector: ConnectorDefinition }) {
  const src = SERVICE_ICONS[connector.id];
  return (
    <div className={`serviceCardIcon serviceIcon-${connector.id}`} aria-hidden="true">
      {src ? <img src={src} alt="" /> : <span>{connector.name[0]}</span>}
    </div>
  );
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
  const [navMode, setNavMode] = useState<NavMode>('all');
  const [listFilter, setListFilter] = useState<ListFilter>('all');
  const [serviceFilter, setServiceFilter] = useState<string | null>(null);
  const [composer, setComposer] = useState('');
  const [showServices, setShowServices] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [connectorStatuses, setConnectorStatuses] = useState<Record<string, ConnectorStatus>>({});
  const [settingsBusyId, setSettingsBusyId] = useState<string | null>(null);
  const [activeConnector, setActiveConnector] = useState<ConnectorDefinition | null>(null);
  const [connectorBusy, setConnectorBusy] = useState(false);
  const [connectorError, setConnectorError] = useState('');
  const [connectorRequirements, setConnectorRequirements] = useState<ConnectorRequirements | null>(null);
  const [connectorSetupValues, setConnectorSetupValues] = useState<Record<string, string>>({});
  const [flows, setFlows] = useState<LoginFlow[]>([]);
  const [loginStep, setLoginStep] = useState<LoginStep | null>(null);
  const [loginValues, setLoginValues] = useState<Record<string, string>>({});
  const [updating, setUpdating] = useState(false);
  const [updateMessage, setUpdateMessage] = useState('Check for updates');
  const searchRef = useRef<HTMLInputElement>(null);

  const visibleRooms = useMemo(() => {
    let next = navMode === 'archive' ? rooms.filter(room => room.archived) : rooms.filter(room => !room.archived);

    if (navMode === 'unread') next = next.filter(room => room.unread > 0);
    if (navMode === 'mentions') next = next.filter(room => room.mentions > 0);
    if (navMode === 'favorites') next = next.filter(room => room.favorite);
    if (serviceFilter) next = next.filter(room => room.service === serviceFilter);

    if (listFilter === 'direct') next = next.filter(room => room.direct);
    if (listFilter === 'groups') next = next.filter(room => !room.direct);
    if (listFilter === 'unread') next = next.filter(room => room.unread > 0);
    if (listFilter === 'favorites') next = next.filter(room => room.favorite);

    const needle = query.trim().toLowerCase();
    if (needle) {
      next = next.filter(room =>
        `${room.name} ${room.service} ${room.preview}`.toLowerCase().includes(needle),
      );
    }
    return next;
  }, [rooms, navMode, listFilter, serviceFilter, query]);

  const selectedRoom = rooms.find(room => room.id === selectedRoomId) ?? visibleRooms[0] ?? rooms[0];
  const networks = [...new Set(rooms.map(room => room.service))].sort();
  const allCount = rooms.filter(room => !room.archived).length;
  const unreadCount = rooms.filter(room => !room.archived && room.unread > 0).length;
  const mentionCount = rooms.filter(room => !room.archived && room.mentions > 0).length;
  const archiveCount = rooms.filter(room => room.archived).length;
  const favoriteCount = rooms.filter(room => !room.archived && room.favorite).length;

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
    if (!selectedRoomId && visibleRooms[0]) setSelectedRoomId(visibleRooms[0].id);
    if (selectedRoomId && rooms.length && !rooms.some(room => room.id === selectedRoomId)) {
      setSelectedRoomId(visibleRooms[0]?.id ?? rooms[0].id);
    }
  }, [rooms, selectedRoomId, visibleRooms]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

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

  async function refreshConnectorStatuses(): Promise<void> {
    const entries = await Promise.all(
      registry.map(async connector => [connector.id, await backend.connectorStatus(connector.id)] as const),
    );
    setConnectorStatuses(Object.fromEntries(entries));
  }

  async function openSettings(): Promise<void> {
    setShowSettings(true);
    try {
      await refreshConnectorStatuses();
    } catch (error) {
      setRuntimeError(errorText(error));
    }
  }

  async function manageConnector(connector: ConnectorDefinition, action: 'start' | 'stop' | 'update'): Promise<void> {
    setSettingsBusyId(connector.id);
    setRuntimeError('');
    try {
      if (action === 'start') await backend.startConnector(connector.id);
      if (action === 'stop') await backend.stopConnector(connector.id);
      if (action === 'update') await backend.updateConnector(connector.id);
      await refreshConnectorStatuses();
    } catch (error) {
      setRuntimeError(errorText(error));
    } finally {
      setSettingsBusyId(null);
    }
  }

  async function connectService(connector: ConnectorDefinition): Promise<void> {
    setActiveConnector(connector);
    setShowServices(false);
    setConnectorBusy(true);
    setConnectorError('');
    setConnectorRequirements(null);
    setConnectorSetupValues({});
    setFlows([]);
    setLoginStep(null);
    setLoginValues({});

    try {
      const requirements = await backend.connectorRequirements(connector.id);
      if (requirements.required) {
        setConnectorRequirements(requirements);
        return;
      }
      await continueConnectService(connector);
    } catch (error) {
      setConnectorError(errorText(error));
    } finally {
      setConnectorBusy(false);
    }
  }

  async function continueConnectService(
    connector: ConnectorDefinition,
    legacyCommandOverride?: string,
  ): Promise<void> {
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

      // Telegram should behave like a normal consumer messenger login.
      // The application-level Telegram API credentials are bundled by Unibox
      // at build time; the user only enters their phone number (and then the
      // Telegram verification code / 2FA password when required).
      if (connector.id === 'telegram') {
        const phoneFlow = nextFlows.find(flow => flow.id === 'phone');
        if (!phoneFlow) {
          throw new Error('Telegram phone-number login is not available in this connector build.');
        }
        await beginFlow(connector, phoneFlow);
        return;
      }

      if (nextFlows.length === 1) await beginFlow(connector, nextFlows[0]);
    } else {
      await beginLegacyLogin(connector, legacyCommandOverride);
    }
  }

  async function submitConnectorRequirements(): Promise<void> {
    if (!activeConnector || !connectorRequirements) return;
    setConnectorBusy(true);
    setConnectorError('');
    try {
      const configured = await backend.configureConnector(activeConnector.id, connectorSetupValues);
      setConnectorRequirements(null);
      setConnectorSetupValues({});
      await continueConnectService(
        activeConnector,
        isProvisioningConnector(activeConnector) ? undefined : configured,
      );
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

  async function beginLegacyLogin(
    connector: ConnectorDefinition,
    commandOverride?: string,
  ): Promise<void> {
    if (!inbox) throw new Error('Local Matrix client is not ready yet.');
    const bot = `@${connector.bot_localpart || `${connector.id}bot`}:unibox.local`;
    const created = await inbox.client.createRoom({
      is_direct: true,
      invite: [bot],
      name: `${connector.name} · Unibox setup`,
    });
    await inbox.client.sendTextMessage(
      created.room_id,
      commandOverride || connector.legacy_login || 'login',
    );
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

  async function selectRoom(roomId: string): Promise<void> {
    setSelectedRoomId(roomId);
    try {
      await inbox?.markRead(roomId);
    } catch (error) {
      setRuntimeError(errorText(error));
    }
  }

  async function sendMessage(): Promise<void> {
    if (!inbox || !selectedRoom || !composer.trim()) return;
    const text = composer;
    setComposer('');
    try {
      await inbox.sendText(selectedRoom.id, text);
      await inbox.markRead(selectedRoom.id);
    } catch (error) {
      setRuntimeError(errorText(error));
      setComposer(text);
    }
  }

  async function toggleFavorite(): Promise<void> {
    if (!inbox || !selectedRoom) return;
    try {
      await inbox.toggleFavorite(selectedRoom.id, !selectedRoom.favorite);
    } catch (error) {
      setRuntimeError(errorText(error));
    }
  }

  async function toggleArchive(): Promise<void> {
    if (!inbox || !selectedRoom) return;
    try {
      await inbox.toggleArchived(selectedRoom.id, !selectedRoom.archived);
      setSelectedRoomId(null);
    } catch (error) {
      setRuntimeError(errorText(error));
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
            Unibox stores its message database, connector sessions, media and settings on this PC
            inside a private native Windows runtime managed automatically by Unibox.
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
            Windows 10/11 x64. No WSL, Docker, Hyper-V, PostgreSQL, Python or separate runtime
            installation is required. The local engine is bundled and managed by Unibox.
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
          <button className={navMode === 'all' ? 'active' : ''} onClick={() => setNavMode('all')}><Inbox size={17} />All Chats <b>{allCount}</b></button>
          <button className={navMode === 'unread' ? 'active' : ''} onClick={() => setNavMode('unread')}><Star size={17} />Unread <b>{unreadCount}</b></button>
          <button className={navMode === 'mentions' ? 'active' : ''} onClick={() => setNavMode('mentions')}><AtSign size={17} />Mentions <b>{mentionCount}</b></button>
          <button className={navMode === 'favorites' ? 'active' : ''} onClick={() => setNavMode('favorites')}><Star size={17} />Favorites <b>{favoriteCount}</b></button>
          <button className={navMode === 'archive' ? 'active' : ''} onClick={() => setNavMode('archive')}><Archive size={17} />Archive <b>{archiveCount}</b></button>
        </nav>
        <div className="sectionTitle">Connected networks</div>
        {networks.slice(0, 10).map(service => (
          <button
            className={serviceFilter === service ? 'service activeService' : 'service'}
            key={service}
            onClick={() => setServiceFilter(current => current === service ? null : service)}
          >
            <span className="serviceDot" />{service}<i />
          </button>
        ))}
        <button className="add" onClick={() => setShowServices(true)}><Plus size={17} />Add service</button>
        <div className="sidebarBottom">
          <button disabled={updating} onClick={() => void runUpdate()}><Download size={16} />{updateMessage}</button>
          <button onClick={() => void openSettings()}><Settings size={16} />Settings</button>
        </div>
      </aside>

      <section className="listPane">
        <div className="search">
          <Search size={17} />
          <input ref={searchRef} value={query} onChange={event => setQuery(event.target.value)} aria-label="Search" placeholder="Search conversations…" />
          <kbd>Ctrl K</kbd>
        </div>
        <div className="filters">
          {(['all', 'direct', 'groups', 'unread', 'favorites'] as ListFilter[]).map(filter => (
            <button key={filter} className={listFilter === filter ? 'selected' : ''} onClick={() => setListFilter(filter)}>
              {filter[0].toUpperCase() + filter.slice(1)}
            </button>
          ))}
        </div>
        {serviceFilter && (
          <div className="activeFilter">Showing {serviceFilter}<button onClick={() => setServiceFilter(null)}><X size={14} /></button></div>
        )}
        <div className="chatList">
          {visibleRooms.length === 0 && <div className="emptyList">No conversations match this view.<br />Add a service or change the filters.</div>}
          {visibleRooms.map(room => (
            <button
              key={room.id}
              onClick={() => void selectRoom(room.id)}
              className={selectedRoom?.id === room.id ? 'chat activeChat' : 'chat'}
            >
              <div className="avatar">{room.name[0]?.toUpperCase() || '?'}</div>
              <div className="chatMeta">
                <strong>{room.name}{room.favorite ? <Star size={12} className="inlineStar" /> : null}</strong>
                <small>{room.service}</small>
                <span>{room.preview}</span>
              </div>
              {room.unread > 0 && <span className="unreadBadge">{room.unread > 99 ? '99+' : room.unread}</span>}
            </button>
          ))}
        </div>
      </section>

      <main className="conversation">
        {selectedRoom ? (
          <>
            <header>
              <div><h2>{selectedRoom.name}</h2><span>{selectedRoom.service}{selectedRoom.unread > 0 ? ` · ${selectedRoom.unread} unread` : ''}</span></div>
              <div className="headerActions">
                <button title={selectedRoom.favorite ? 'Remove from favorites' : 'Add to favorites'} className={selectedRoom.favorite ? 'selectedAction' : ''} onClick={() => void toggleFavorite()}><Star size={18} /></button>
                <button title={selectedRoom.archived ? 'Restore conversation' : 'Archive conversation'} onClick={() => void toggleArchive()}><Archive size={18} /></button>
                <button title="Conversation options"><MoreHorizontal size={18} /></button>
              </div>
            </header>
            {runtimeError && <div className="conversationError"><ErrorBox text={runtimeError} /></div>}
            <div className="messages">
              {selectedRoom.messages.map(message => (
                <div key={message.id} className={message.mine ? 'bubble mine' : 'bubble'}>
                  <b>{message.mine ? 'You' : message.sender}</b>
                  {message.msgtype === 'm.image' && message.mediaUrl ? (
                    <img
                      className="messageImage"
                      src={message.mediaUrl}
                      alt={message.body || 'Bridge QR code'}
                    />
                  ) : (
                    <p>{message.body || `[${message.msgtype.replace('m.', '')}]`}</p>
                  )}
                  <div className="bubbleFooter">
                    <small>{new Date(message.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</small>
                    <div className="bubbleActions">
                      <button title="React with thumbs up" onClick={() => void inbox?.react(selectedRoom.id, message.id, '👍')}>👍</button>
                      {message.mine && <button title="Delete message" onClick={() => void inbox?.deleteMessage(selectedRoom.id, message.id)}><Trash2 size={13} /></button>}
                    </div>
                  </div>
                </div>
              ))}
            </div>
            <div className="composer">
              <button aria-label="Add attachment" title="Attachments are coming in the next media capability pass"><Plus size={18} /></button>
              <input
                value={composer}
                onChange={event => setComposer(event.target.value)}
                onFocus={() => void inbox?.markRead(selectedRoom.id)}
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
                  <ServiceIcon connector={connector} />
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

      {showSettings && (
        <div className="modalBackdrop" onMouseDown={() => setShowSettings(false)}>
          <div className="modal settingsModal" onMouseDown={event => event.stopPropagation()}>
            <div className="modalHeader">
              <div><h2>Settings</h2><p>Local engine, connector health and updates.</p></div>
              <button onClick={() => setShowSettings(false)}><X size={20} /></button>
            </div>
            <div className="privacyCard settingsPrivacy">
              <strong>Local-first by design</strong>
              <span>The native Matrix engine, connector sessions, message history and media live on this device. Unibox has no central chat-storage service.</span>
            </div>
            <div className="settingsSection">
              <h3>Local engine</h3>
              <div className="settingsRow"><span>Local Matrix engine</span><b>{runtime.synapse_ready ? 'Running' : 'Stopped'}</b></div>
              <div className="settingsRow"><span>Matrix session</span><b>{runtime.matrix_session_ready ? 'Ready' : 'Missing'}</b></div>
              <div className="settingsRow"><span>Data</span><code>{runtime.data_root}</code></div>
            </div>
            <div className="settingsSection">
              <div className="settingsTitleRow"><h3>Connectors</h3><button onClick={() => void refreshConnectorStatuses()}><RefreshCw size={15} />Refresh</button></div>
              <div className="connectorSettingsList">
                {registry.filter(item => connectorStatuses[item.id]?.installed).map(connector => {
                  const status = connectorStatuses[connector.id];
                  const busy = settingsBusyId === connector.id;
                  return (
                    <div className="connectorSettingsRow" key={connector.id}>
                      <div><strong>{connector.name}</strong><span>{status?.running ? 'Running' : 'Stopped'} · {connector.maturity}</span></div>
                      <div>
                        <button disabled={busy} onClick={() => void manageConnector(connector, status?.running ? 'stop' : 'start')}>{status?.running ? 'Stop' : 'Start'}</button>
                        <button disabled={busy} onClick={() => void manageConnector(connector, 'update')}>{busy ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}Update</button>
                      </div>
                    </div>
                  );
                })}
                {!registry.some(item => connectorStatuses[item.id]?.installed) && <div className="emptyList">No connectors installed yet.</div>}
              </div>
            </div>
            <button className="primaryButton settingsUpdate" disabled={updating} onClick={() => void runUpdate()}><Download size={17} />{updateMessage}</button>
          </div>
        </div>
      )}

      {activeConnector && (
        <div className="modalBackdrop">
          <div className="modal loginModal">
            <div className="modalHeader">
              <div><h2>Connect {activeConnector.name}</h2><p>{activeConnector.description}</p></div>
              <button onClick={() => { setActiveConnector(null); setConnectorRequirements(null); setConnectorSetupValues({}); }}><X size={20} /></button>
            </div>
            {activeConnector.risk_notice && <WarningBox text={activeConnector.risk_notice} />}
            {connectorError && <ErrorBox text={connectorError} />}
            {connectorRequirements && (
              <div className="loginForm connectorRequirements">
                {connectorRequirements.help && <p>{connectorRequirements.help}</p>}
                {connectorRequirements.fields.map(field => (
                  <label key={field.id}>
                    <span>{field.name}</span>
                    {field.description && <small>{field.description}</small>}
                    <input
                      type={field.type}
                      autoComplete="off"
                      value={connectorSetupValues[field.id] ?? ''}
                      onChange={event => setConnectorSetupValues({
                        ...connectorSetupValues,
                        [field.id]: event.target.value,
                      })}
                    />
                  </label>
                ))}
                <button
                  className="primaryButton"
                  disabled={connectorBusy || connectorRequirements.fields.some(field => !(connectorSetupValues[field.id] ?? '').trim())}
                  onClick={() => void submitConnectorRequirements()}
                >
                  {connectorBusy ? <LoaderCircle className="spin" size={18} /> : null}
                  Continue
                </button>
              </div>
            )}
            {connectorBusy && !connectorRequirements && <div className="loadingRow"><LoaderCircle className="spin" size={20} />Preparing connector…</div>}
            {!connectorBusy && !connectorRequirements && !loginStep && flows.length > 1 && (
              <div className="flowList">
                {flows.map(flow => (
                  <button key={flow.id} onClick={() => void beginFlow(activeConnector, flow)}>
                    <strong>{flow.name}</strong><span>{flow.description}</span>
                  </button>
                ))}
              </div>
            )}
            {!connectorBusy && !connectorRequirements && !loginStep && flows.length === 0 && !connectorError && (
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
