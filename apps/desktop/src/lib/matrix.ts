import { createClient, type MatrixClient, type MatrixEvent, type Room } from 'matrix-js-sdk';
import type { MatrixSession } from './backend';

export type InboxMessage = {
  id: string;
  sender: string;
  body: string;
  timestamp: number;
  mine: boolean;
};

export type InboxRoom = {
  id: string;
  name: string;
  service: string;
  preview: string;
  timestamp: number;
  messages: InboxMessage[];
};

export type InboxController = {
  client: MatrixClient;
  sendText: (roomId: string, body: string) => Promise<void>;
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

function roomSnapshot(client: MatrixClient, room: Room): InboxRoom {
  const events = room
    .getLiveTimeline()
    .getEvents()
    .filter(event => event.getType() === 'm.room.message')
    .slice(-80);

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

  await client.startClient({ initialSyncLimit: 40, lazyLoadMembers: true });

  const refresh = () => {
    const rooms = client
      .getRooms()
      .map(room => roomSnapshot(client, room))
      .sort((a, b) => b.timestamp - a.timestamp || a.name.localeCompare(b.name));
    onRooms(rooms);
  };

  refresh();
  const timer = window.setInterval(refresh, 1000);

  return {
    client,
    sendText: async (roomId: string, body: string) => {
      const text = body.trim();
      if (!text) return;
      await client.sendTextMessage(roomId, text);
      refresh();
    },
    stop: () => {
      window.clearInterval(timer);
      client.stopClient();
    },
  };
}
