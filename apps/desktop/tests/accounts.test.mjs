import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  buildCancelLoginPath,
  buildLoginStartPath,
  buildLogoutPath,
  parseConnectorLoginIds,
} from '../src/lib/accounts.ts';

test('connector login ids are normalized and deduplicated', () => {
  assert.deepEqual(
    parseConnectorLoginIds({ login_ids: [' one ', 'two', 'one', '', 4, null] }),
    ['one', 'two'],
  );
  assert.deepEqual(parseConnectorLoginIds({ login_ids: 'one' }), []);
  assert.deepEqual(parseConnectorLoginIds(null), []);
});

test('re-login paths preserve opaque ids safely', () => {
  assert.equal(
    buildLoginStartPath('qr flow', 'acct/one + two'),
    '/v3/login/start/qr%20flow?client_http=1&login_id=acct%2Fone+%2B+two',
  );
  assert.equal(buildLoginStartPath('qr'), '/v3/login/start/qr?client_http=1');
});

test('destructive lifecycle paths require and encode ids', () => {
  assert.equal(buildLogoutPath('account/a'), '/v3/logout/account%2Fa');
  assert.equal(buildCancelLoginPath('process/a'), '/v3/login/cancel/process%2Fa');
  assert.throws(() => buildLogoutPath('   '), /login ID must not be empty/);
});
