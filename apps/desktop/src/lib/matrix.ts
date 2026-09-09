import {
  NotificationCountType,
  createClient,
  type MatrixClient,
  type MatrixEvent,
  type Room,
} from 'matrix-js-sdk';
import type { MatrixSession } from './backend';

const FAVORITE_TAG = 'm.favourite';
const ARCHIVE_TAG = 'u.unibox.archive';

export type InboxMessage = {
  id: string;
  sender: string;
  body: string;
  timestamp: number;
  mine: boolean;
  msgtype: string;
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
  direct: boolean;
};

export type InboxController = {
  client: MatrixClient;
  sendText: (roomId: string, body: string) => Promise<void>;
  markRead: (roomId: string) => Promise<void>;
  toggleFavorite: (roomId: string, favorite: boolean) => Promise<void>;
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
  const event = client.getAccountData('m.direct');
  const content = event?.getContent() as Record<string, unknown> | undefined;
  if (!content) return false;
  return Object.values(content).some(value =>
    Array.isArray(value) && value.some(candidate => candidate === roomId),
  );
}

function roomSnapshot(client: MatrixClient, room: Room): InboxRoom {
  const events = room
    .getLiveTimeline()
    .getEvents()
    .filter(event => event.getType() === 'm.room.message')
    .slice(-100);

  const messages: InboxMessage[] = events.map(event => {
    const content = event.getContent() as { body?: string; msgtype?: string };
    const senderId = event.getSender() ?? '';
    const sender = room.getMember(senderId)?.name || senderId || 'Unknown';
    return {
      id: event.getId() ?? `${event.getTs()}-${senderId}`,
      sender,
      body: typeof content.body === 'string' ? content.body : '',
      timestamp: event.getTs(),
      mine: senderId === client.getUserId(),
      msgtype: typeof content.msgtype === 'string' ? content.msgtype : 'm.text',
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
    direct: isDirectRoom(client, room.roomId),
  };
}

export async function startInbox(
  session: MatrixSession,
  onRooms: (rooms: InboxRoom[]) => void,
): Promise<InboxController> {
  const client = createClient({
    baseUrl: session.homeserver,
    accessToken: session.access_token,
    userId: session.user_id,
    deviceId: session.device_id ?? undefined,
    timelineSupport: true,
  });

  await client.startClient({ initialSyncLimit: 50, lazyLoadMembers: true });

  const refresh = () => {
    const rooms = client
      .getRooms()
      .filter(room => room.getMyMembership() === 'join')
      .map(room => roomSnapshot(client, room))
      .sort((a, b) => b.timestamp - a.timestamp || a.name.localeCompare(b.name));
    onRooms(rooms);
  };

  refresh();
  const timer = window.setInterval(refresh, 750);

  const latestReadableEvent = (roomId: string): MatrixEvent | undefined => {
    const room = client.getRoom(roomId);
    if (!room) return undefined;
    return [...room.getLiveTimeline().getEvents()]
      .reverse()
      .find(event => Boolean(event.getId()) && event.getType() === 'm.room.message');
  };

  return {
    client,
    sendText: async (roomId: string, body: string) => {
      const text = body.trim();
      if (!text) return;
      await client.sendTextMessage(roomId, text);
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
      window.clearInterval(timer);
      client.stopClient();
    },
  };
}
