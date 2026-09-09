import { test } from 'node:test';
import assert from 'node:assert/strict';
import { roomSnapshot } from '../src/lib/matrix.ts';

const event = (id, content = {}, options = {}) => ({
  getId: () => id, getTs: () => 1000, getSender: () => '@me:local',
  getType: () => options.type ?? 'm.room.message',
  getContent: () => ({ body: id, msgtype: 'm.text', ...content }),
  getRelation: () => content['m.relates_to'],
  isRedacted: () => options.redacted ?? false,
  replacingEvent: () => options.edited ?? null,
});
const client = { getUserId: () => '@me:local', getAccountData: () => undefined };
const room = events => ({
  roomId: '!room:local', name: 'Test room', tags: {},
  getLiveTimeline: () => ({ getEvents: () => events }),
  getMember: () => ({ name: 'Me' }),
  getUsersReadUpTo: () => ['@other:local'],
  getUnreadNotificationCount: () => 0,
  getJoinedMembers: () => [{ userId: '@other:local', name: 'Other', typing: true }],
  currentState: { getStateEvents: () => [] },
});

test('redacted messages and edit transport events are excluded', () => {
  const snapshot = roomSnapshot(client, room([
    event('$visible'), event('$deleted', {}, { redacted: true }),
    event('$edit', { 'm.relates_to': { rel_type: 'm.replace', event_id: '$visible' } }),
  ]));
  assert.deepEqual(snapshot.messages.map(message => message.id), ['$visible']);
});

test('reactions, replies, read receipts and typing are reflected', () => {
  const snapshot = roomSnapshot(client, room([
    event('$reply', { 'm.relates_to': { 'm.in_reply_to': { event_id: '$original' } } }),
    event('$reaction', { 'm.relates_to': { rel_type: 'm.annotation', event_id: '$reply', key: '👍' } }, { type: 'm.reaction' }),
    event('$removed-reaction', { 'm.relates_to': { rel_type: 'm.annotation', event_id: '$reply', key: '👍' } }, { type: 'm.reaction', redacted: true }),
  ]));
  assert.equal(snapshot.messages[0].replyTo, '$original');
  assert.deepEqual(snapshot.messages[0].reactions, [{ key: '👍', count: 1 }]);
  assert.equal(snapshot.messages[0].read, true);
  assert.deepEqual(snapshot.typing, ['Other']);
});

test('history rendering is bounded and can expand when history is requested', () => {
  const history = room(Array.from({ length: 200 }, (_, index) => event(`$${index}`)));
  assert.equal(roomSnapshot(client, history).messages.length, 100);
  assert.equal(roomSnapshot(client, history, 150).messages.length, 150);
});
