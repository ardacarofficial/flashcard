import { describe, expect, it } from 'vitest';
import { KEYS, MemoryKv } from '../../src/lib/kv';
import { Service, type Pairing } from '../../src/lib/service';
import { MockApi, sampleData } from '../mock-api/handler';

const ORIGIN = 'chrome-extension://abcdefghijklmnop';

function setup(opts: { autoApprove?: number } = {}) {
  const mock = new MockApi();
  sampleData(mock);
  mock.autoApproveAfterPolls = opts.autoApprove ?? 1;
  const state = { online: true };
  const local = new MemoryKv();
  let n = 0;
  const service = new Service({
    local,
    fetch: mock.fetcher({ origin: ORIGIN, online: () => state.online }),
    now: () => 1000,
    uuid: () => `ref-${++n}`,
    clientName: 'Tarayıcı eklentisi',
  });
  return { mock, state, local, service };
}

async function pair(s: ReturnType<typeof setup>) {
  await s.service.setPort(s.mock.port);
  const started = await s.service.startPairing();
  expect(started.ok).toBe(true);
  expect(await s.service.pollPairing()).toEqual({ ok: true, value: 'paired' });
}

describe('pairing', () => {
  it('waits for approval in the app, then stores the token from the first approved answer', async () => {
    const t = setup({ autoApprove: -1 });
    await t.service.startPairing();
    expect((await t.service.status()).pairing).toBe('pending');
    expect(await t.service.pollPairing()).toEqual({ ok: true, value: 'pending' });
    t.mock.approve([...t.mock.clients.keys()][0]!);
    expect(await t.service.pollPairing()).toEqual({ ok: true, value: 'paired' });
    const p = await t.local.get<Pairing>(KEYS.pairing);
    expect(p?.token).toMatch(/^tok-/);
    expect(p?.scopes).toEqual(['inbox:write', 'decks:read', 'attributes:read']);
    expect((await t.service.status())).toMatchObject({ app: 'online', paired: true, pairing: 'paired' });
  });

  it('handles denial, expiry (404) and an approved answer whose token was already handed out', async () => {
    const s = setup({ autoApprove: -1 });
    await s.service.startPairing();
    s.mock.clients.forEach((c) => (c.status = 'denied'));
    expect(await s.service.pollPairing()).toEqual({ ok: true, value: 'denied' });
    expect((await s.service.status()).last).toBe('denied');

    await s.service.startPairing();
    s.mock.clients.clear(); // app restarted: pending requests are gone
    expect(await s.service.pollPairing()).toEqual({ ok: true, value: 'expired' });

    await s.service.startPairing();
    const c = [...s.mock.clients.values()][0]!;
    c.status = 'approved';
    c.tokenGiven = true;
    expect(await s.service.pollPairing()).toEqual({ ok: true, value: 'expired' });
    expect((await s.service.status()).paired).toBe(false);
  });

  it('forgets a revoked token on the next call but keeps queued items', async () => {
    const s = setup();
    await pair(s);
    s.mock.revokeAll();
    expect(await s.service.submit({ front: 'kept' })).toEqual({ status: 'queued', reason: 'unpaired' });
    expect(await s.service.status()).toMatchObject({ paired: false, last: 'revoked', outbox: { waiting: 1, failed: 0 } });
  });
});

describe('adding a word (acceptance: a word added from the extension lands in the inbox)', () => {
  it('sends front, deck, classification and page context to the inbox', async () => {
    const s = setup();
    await pair(s);
    const r = await s.service.submit({
      front: 'badger',
      back: 'porsuk',
      deck_id: 'd-en',
      attributes: { 'attr-seviye': 'opt-B1' },
      context: { url: 'https://e.x/b', title: 'Badgers', sentence: 'The badger dug a hole.' },
    });
    expect(r).toEqual({ status: 'sent' });
    expect(s.mock.inbox).toHaveLength(1);
    expect(s.mock.inbox[0]!.item).toEqual({
      front: 'badger',
      back: 'porsuk',
      deck_id: 'd-en',
      attributes: { 'attr-seviye': 'opt-B1' },
      context: { url: 'https://e.x/b', title: 'Badgers', sentence: 'The badger dug a hole.' },
      client_ref: 'ref-1',
    });
  });

  it('queues while the app is closed and delivers exactly once, in order, when it opens', async () => {
    const s = setup();
    await pair(s);
    s.state.online = false;
    expect(await s.service.submit({ front: 'one' })).toEqual({ status: 'queued', reason: 'offline' });
    expect(await s.service.submit({ front: 'two' })).toEqual({ status: 'queued', reason: 'offline' });
    expect(s.mock.inbox).toHaveLength(0);
    s.state.online = true;
    expect(await s.service.flush()).toEqual({ sent: 2, failed: 0 });
    expect(await s.service.flush()).toEqual({ sent: 0, failed: 0 });
    expect(s.mock.inbox.map((i) => i.item.front)).toEqual(['one', 'two']);
    // a new add goes after older queued ones
    s.state.online = false;
    await s.service.submit({ front: 'three' });
    s.state.online = true;
    await s.service.submit({ front: 'four' });
    expect(s.mock.inbox.map((i) => i.item.front)).toEqual(['one', 'two', 'three', 'four']);
  });

  it('queues while the inbox module is off (503) and reports a rejection without queueing', async () => {
    const s = setup();
    await pair(s);
    s.mock.inboxDisabled = true;
    expect(await s.service.submit({ front: 'later' })).toEqual({ status: 'queued', reason: 'busy' });
    s.mock.inboxDisabled = false;
    const r = await s.service.submit({ front: 'x', deck_id: 'deleted' });
    expect(r.status === 'rejected' && r.error.code).toBe('NOT_FOUND_DECK');
    expect(s.mock.inbox.map((i) => i.item.front)).toEqual(['later']);
    expect((await s.service.status()).outbox).toEqual({ waiting: 0, failed: 0 });
  });

  it('a resend after a lost response does not duplicate the inbox item', async () => {
    const s = setup();
    await pair(s);
    // The server stores the item but the answer never arrives.
    const real = s.mock.handle.bind(s.mock);
    let drop = true;
    s.mock.handle = (req) => {
      const res = real(req);
      if (drop && req.path === '/v1/inbox') {
        drop = false;
        throw new TypeError('connection reset');
      }
      return res;
    };
    expect((await s.service.submit({ front: 'once' })).status).toBe('queued');
    await s.service.flush();
    expect(s.mock.inbox).toHaveLength(1);
  });

  it('shows the last known decks and classifications while the app is closed', async () => {
    const s = setup();
    await pair(s);
    expect((await s.service.decks()).cached).toBe(false);
    expect((await s.service.classifications('d-en')).list).toHaveLength(1);
    s.state.online = false;
    const d = await s.service.decks();
    expect(d.cached).toBe(true);
    expect(d.decks.map((x) => x.name)).toContain('İngilizce');
    expect((await s.service.classifications('d-en')).list[0]!.def.name).toBe('Seviye');
    expect((await s.service.classifications('d-unknown')).list).toEqual([]);
  });

  it('keeps a valid port setting and ignores invalid ones', async () => {
    const s = setup();
    expect((await s.service.settings()).port).toBe(28735);
    expect(await s.service.setPort(31000)).toEqual({ port: 31000 });
    expect(await s.service.setPort(70000)).toEqual({ port: 31000 });
  });
});
