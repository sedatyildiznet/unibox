import {
  NotificationCountType,
  RelationType,
  MsgType,
  ClientEvent,
  RoomEvent,
  RoomMemberEvent,
  createClient,
  type MatrixClient,
  type MatrixEvent,
  type Room,
} from 'matrix-js-sdk';
import type { MatrixSession } from './backend';

const FAVORITE_TAG = 'm.favourite';
const ARCHIVE_TAG = 'u.unibox.archive';
const MUTE_TAG = 'u.unibox.muted';

export type InboxMessage = {
  id: string;
  sender: string;
  body: string;
  timestamp: number;
  mine: boolean;
  msgtype: string;
  url?: string;
  mimetype?: string;
  size?: number;
  replyTo?: string;
  edited: boolean;
  reactions: Array<{ key: string; count: number }>;
  read: boolean;
};

export type InboxRoom = {
  id: string;
  name: string;
  service: string;
  preview: string;
  timestamp: number;
  messages: InboxMessage[];
  unread: number;
  mentions: number;
  favorite: boolean;
  archived: boolean;
  muted: boolean;
  direct: boolean;
  typing: string[];
};

export type InboxController = {
  client: MatrixClient;
  sendText: (roomId: string, body: string, replyTo?: string) => Promise<void>;
  editMessage: (roomId: string, eventId: string, body: string) => Promise<void>;
  sendAttachment: (roomId: string, file: File) => Promise<void>;
  downloadMedia: (mxc: string) => Promise<Blob>;
  loadEarlier: (roomId: string) => Promise<void>;
  markRead: (roomId: string) => Promise<void>;
  toggleFavorite: (roomId: string, favorite: boolean) => Promise<void>;
  toggleMuted: (roomId: string, muted: boolean) => Promise<void>;
  toggleArchived: (roomId: string, archived: boolean) => Promise<void>;
  react: (roomId: string, eventId: string, key: string) => Promise<void>;
  deleteMessage: (roomId: string, eventId: string) => Promise<void>;
  stop: () => void;
};

function serviceName(room: Room): string {
  try {
    const bridgeEvents = room.currentState.getStateEvents('m.bridge') as MatrixEvent[];
    const content = bridgeEvents?.[0]?.getContent() as Record<string, unknown> | undefined;
    const protocol = content?.protocol as Record<string, unknown> | undefined;
    const display = protocol?.displayname ?? protocol?.id;
    if (typeof display === 'string' && display.trim()) return display;
    const network = content?.network;
    if (typeof network === 'string' && network.trim()) return network;
  } catch {
    // Some legacy bridges do not emit m.bridge state. The room still works normally.
  }
  return 'Connected service';
}

function isDirectRoom(client: MatrixClient, roomId: string): boolean {
  // Some matrix-js-sdk releases omit m.direct from AccountDataEvents typing even though
  // m.direct is standard Matrix account data. Keep this narrow compatibility cast local.
  const getAccountData = client.getAccountData.bind(client) as unknown as (
    eventType: string,
  ) => MatrixEvent | undefined;
  const event = getAccountData('m.direct');
  const content = event?.getContent() as Record<string, unknown> | undefined;
  if (!content) return false;
  return Object.values(content).some(
    value => Array.isArray(value) && value.some(candidate => candidate === roomId),
  );
}

export function roomSnapshot(client: MatrixClient, room: Room, limit = 100): InboxRoom {
  const events = room
    .getLiveTimeline()
    .getEvents()
    .filter(event => event.getType() === 'm.room.message' && !event.isRedacted() && event.getRelation()?.rel_type !== RelationType.Replace)
    .slice(-limit);

  const reactionCounts = new Map<string, Map<string, number>>();
  for (const reaction of room.getLiveTimeline().getEvents()) {
    const rel = reaction.getRelation();
    if (!reaction.isRedacted() && rel?.rel_type === RelationType.Annotation && typeof rel.event_id === 'string' && typeof rel.key === 'string') {
      const counts = reactionCounts.get(rel.event_id) ?? new Map<string, number>();
      counts.set(rel.key, (counts.get(rel.key) ?? 0) + 1);
      reactionCounts.set(rel.event_id, counts);
    }
  }
  const messages: InboxMessage[] = events.map(event => {
    const content = event.getContent();
    const relation = content['m.relates_to'];
    const counts = reactionCounts.get(event.getId() ?? '') ?? new Map<string, number>();
    const senderId = event.getSender() ?? '';
    const sender = room.getMember(senderId)?.name || senderId || 'Unknown';
    return {
      id: event.getId() ?? `${event.getTs()}-${senderId}`,
      sender,
      body: typeof content.body === 'string' ? content.body : '',
      timestamp: event.getTs(),
      mine: senderId === client.getUserId(),
      msgtype: typeof content.msgtype === 'string' ? content.msgtype : 'm.text',
      url: typeof content.url === 'string' ? content.url : undefined,
      mimetype: content.info?.mimetype,
      size: content.info?.size,
      replyTo: relation?.['m.in_reply_to']?.event_id,
      edited: Boolean(event.replacingEvent()),
      reactions: [...counts].map(([key, count]) => ({ key, count })),
      read: room.getUsersReadUpTo(event).some(user => user !== client.getUserId()),
    };
  });

  const last = messages.at(-1);
  return {
    id: room.roomId,
    name: room.name || 'Conversation',
    service: serviceName(room),
    preview: last?.body || 'No messages yet',
    timestamp: last?.timestamp || 0,
    messages,
    unread: room.getUnreadNotificationCount(NotificationCountType.Total) || 0,
    mentions: room.getUnreadNotificationCount(NotificationCountType.Highlight) || 0,
    favorite: Boolean(room.tags[FAVORITE_TAG]),
    archived: Boolean(room.tags[ARCHIVE_TAG]),
    muted: Boolean(room.tags[MUTE_TAG]),
    direct: isDirectRoom(client, room.roomId),
    typing: room.getJoinedMembers().filter(member => member.typing && member.userId !== client.getUserId()).map(member => member.name),
  };
}

export async function startInbox(
  session: MatrixSession,
  onRooms: (rooms: InboxRoom[]) => void,
  onMessage?: (roomId: string, name: string) => void,
): Promise<InboxController> {
  const client = createClient({
    baseUrl: session.homeserver,
    accessToken: session.access_token,
    userId: session.user_id,
    deviceId: session.device_id ?? undefined,
    timelineSupport: true,
  });

  let initialSyncComplete = false;
  client.on(ClientEvent.Sync, state => { if (state === 'PREPARED') initialSyncComplete = true; });
  client.on(RoomEvent.Timeline, (event, room, toStartOfTimeline, removed, data) => {
    if (initialSyncComplete && data.liveEvent && !toStartOfTimeline && !removed && room &&
        event.getType() === 'm.room.message' && event.getSender() !== session.user_id &&
        !event.isRedacted() && event.getRelation()?.rel_type !== RelationType.Replace && !room.tags[MUTE_TAG]) {
      onMessage?.(room.roomId, room.name || 'New message');
    }
  });
  await client.startClient({ initialSyncLimit: 50, lazyLoadMembers: true });

  const historyLimits = new Map<string, number>();
  const refresh = () => {
    const rooms = client
      .getRooms()
      .filter(room => room.getMyMembership() === 'join')
      .map(room => roomSnapshot(client, room, historyLimits.get(room.roomId) ?? 100))
      .sort((a, b) => b.timestamp - a.timestamp || a.name.localeCompare(b.name));
    onRooms(rooms);
  };

  refresh();
  let refreshTimer: number | undefined;
  const scheduleRefresh = () => {
    if (refreshTimer !== undefined) return;
    refreshTimer = window.setTimeout(() => { refreshTimer = undefined; refresh(); }, 100);
  };
  const refreshEvents = [ClientEvent.Sync, ClientEvent.AccountData, RoomEvent.Timeline, RoomEvent.Receipt, RoomEvent.Tags, RoomEvent.Redaction, RoomMemberEvent.Typing] as const;
  for (const event of refreshEvents) client.on(event, scheduleRefresh);

  const latestReadableEvent = (roomId: string): MatrixEvent | undefined => {
    const room = client.getRoom(roomId);
    if (!room) return undefined;
    return [...room.getLiveTimeline().getEvents()]
      .reverse()
      .find(event => Boolean(event.getId()) && event.getType() === 'm.room.message');
  };

  return {
    client,
    sendText: async (roomId: string, body: string, replyTo?: string) => {
      const text = body.trim();
      if (!text) return;
      await client.sendMessage(roomId, {
        msgtype: MsgType.Text, body: text,
        ...(replyTo ? { 'm.relates_to': { 'm.in_reply_to': { event_id: replyTo } } } : {}),
      });
      refresh();
    },
    editMessage: async (roomId, eventId, body) => {
      await client.sendMessage(roomId, {
        msgtype: MsgType.Text, body: `* ${body}`,
        'm.new_content': { msgtype: MsgType.Text, body },
        'm.relates_to': { rel_type: RelationType.Replace, event_id: eventId },
      });
      refresh();
    },
    sendAttachment: async (roomId, file) => {
      if (file.size > 25 * 1024 * 1024) throw new Error('Attachments must be 25 MB or smaller.');
      const upload = await client.uploadContent(file, { name: file.name, type: file.type || 'application/octet-stream' });
      const media = { body: file.name, url: upload.content_uri };
      if (file.type.startsWith('image/')) await client.sendMessage(roomId, { ...media, msgtype: MsgType.Image });
      else if (file.type.startsWith('video/')) await client.sendMessage(roomId, { ...media, msgtype: MsgType.Video });
      else if (file.type.startsWith('audio/')) await client.sendMessage(roomId, { ...media, msgtype: MsgType.Audio });
      else await client.sendMessage(roomId, { ...media, msgtype: MsgType.File, info: { mimetype: file.type || 'application/octet-stream', size: file.size } });
      refresh();
    },
    downloadMedia: async mxc => {
      if (!mxc.startsWith('mxc://')) throw new Error('Invalid media reference.');
      const url = client.mxcUrlToHttp(mxc, undefined, undefined, undefined, false, false, true);
      if (!url || new URL(url).origin !== new URL(session.homeserver).origin) throw new Error('Invalid media endpoint.');
      const response = await fetch(url, { headers: { Authorization: `Bearer ${session.access_token}` }, redirect: 'error' });
      if (!response.ok) throw new Error('Media download failed.');
      return response.blob();
    },
    loadEarlier: async roomId => {
      const room = client.getRoom(roomId);
      if (room) {
        await client.scrollback(room, 50);
        historyLimits.set(roomId, Math.min(500, (historyLimits.get(roomId) ?? 100) + 50));
      }
      refresh();
    },
    markRead: async (roomId: string) => {
      const event = latestReadableEvent(roomId);
      const eventId = event?.getId();
      if (!event || !eventId) return;
      await client.setRoomReadMarkers(roomId, eventId, event);
      refresh();
    },
    toggleFavorite: async (roomId: string, favorite: boolean) => {
      if (favorite) await client.setRoomTag(roomId, FAVORITE_TAG, {});
      else await client.deleteRoomTag(roomId, FAVORITE_TAG);
      refresh();
    },
    toggleMuted: async (roomId, muted) => {
      if (muted) await client.setRoomTag(roomId, MUTE_TAG, {});
      else await client.deleteRoomTag(roomId, MUTE_TAG);
      refresh();
    },
    toggleArchived: async (roomId: string, archived: boolean) => {
      if (archived) await client.setRoomTag(roomId, ARCHIVE_TAG, {});
      else await client.deleteRoomTag(roomId, ARCHIVE_TAG);
      refresh();
    },
    react: async (roomId: string, eventId: string, key: string) => {
      await client.sendEvent(
        roomId,
        'm.reaction' as never,
        {
          'm.relates_to': {
            rel_type: 'm.annotation',
            event_id: eventId,
            key,
          },
        } as never,
      );
      refresh();
    },
    deleteMessage: async (roomId: string, eventId: string) => {
      await client.redactEvent(roomId, eventId);
      refresh();
    },
    stop: () => {
      window.clearTimeout(refreshTimer);
      for (const event of refreshEvents) client.removeListener(event, scheduleRefresh);
      client.stopClient();
    },
  };
}
