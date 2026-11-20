import { inboxItem } from './item';
import { describe, expect, it } from 'vitest';
import { LocalApi, isTransient, rejection } from '../../src/lib/api';
import { DEFAULT_PORT } from '../../src/lib/contract';
import { MockApi, sampleData } from '../mock-api/handler';

const ORIGIN = 'chrome-extension://abcdefghijklmnop';

function setup() {
  const mock = new MockApi();
  sampleData(mock);
  const fetch = mock.fetcher({ origin: ORIGIN });
  return { mock, fetch };
}

async function paired() {
  const { mock, fetch } = setup();
  const anon = new LocalApi({ port: mock.port, fetch });
  const started = await anon.requestPairing('Tarayıcı eklentisi');
  if (!started.ok) throw new Error('pair failed');
  const st = await anon.pairingStatus(started.value.request_id);
  if (!st.ok || !st.value.token) throw new Error('no token');
  return { mock, fetch, api: new LocalApi({ port: mock.port, fetch, token: st.value.token }) };
}

describe('LocalApi', () => {
  it('uses the documented default port and only 127.0.0.1', () => {
    expect(DEFAULT_PORT).toBe(28735);
    expect(new LocalApi({ port: DEFAULT_PORT }).baseUrl).toBe('http://127.0.0.1:28735');
  });

  it('pings without a token and treats a foreign service as not the app', async () => {
    const { mock, fetch } = setup();
    expect((await new LocalApi({ port: mock.port, fetch }).ping()).ok).toBe(true);
    const foreign = (async () => new Response(JSON.stringify({ app: 'something-else' }), { status: 200 })) as typeof globalThis.fetch;
    expect(await new LocalApi({ port: 1, fetch: foreign }).ping()).toEqual({ ok: false, failure: { kind: 'protocol', status: 200 } });
  });

  it('reports a closed app as offline (not an exception)', async () => {
    const { mock } = setup();
    const down = mock.fetcher({ online: () => false });
    const r = await new LocalApi({ port: mock.port, fetch: down }).ping();
    expect(r).toEqual({ ok: false, failure: { kind: 'offline' } });
  });

  it('pairs: requests the scopes it needs, token only in the first approved answer', async () => {
    const { mock, fetch } = setup();
    const api = new LocalApi({ port: mock.port, fetch });
    const r = await api.requestPairing('Browser extension');
    expect(r.ok && r.value.status).toBe('pending');
    const body = JSON.parse(mock.log.at(-1)!.body!);
    expect(body).toEqual({ client_name: 'Browser extension', client_kind: 'browser_extension', scopes: ['inbox:write', 'decks:read', 'attributes:read'] });
    expect(mock.log.at(-1)!.headers.Authorization).toBeUndefined();
    const id = r.ok ? r.value.request_id : '';
    const first = await api.pairingStatus(id);
    const second = await api.pairingStatus(id);
    expect(first.ok && first.value.token).toBeTruthy();
    expect(second.ok && second.value.token).toBeUndefined();
    // Track I's CSRF guard: polling is a POST; every call but ping names the client and every
    // POST is JSON.
    expect(mock.log.at(-1)!.method).toBe('POST');
    for (const req of mock.log) {
      expect(req.headers['X-Flashcard-Client']).toBe('flashcard-browser-extension');
      if (req.method === 'POST') expect(req.headers['Content-Type']).toBe('application/json');
    }
  });

  it('needs a token for everything else; a missing/revoked token is "unauthorized"', async () => {
    const { mock, fetch } = setup();
    expect(await new LocalApi({ port: mock.port, fetch }).decks()).toEqual({ ok: false, failure: { kind: 'unauthorized' } });
    const bad = await new LocalApi({ port: mock.port, fetch, token: 'nope' }).decks();
    expect(!bad.ok && bad.failure.kind).toBe('unauthorized');
    const { mock: m2, api } = await paired();
    m2.revokeAll();
    const revoked = await api.addToInbox(inboxItem({ front: 'x' }));
    expect(!revoked.ok && revoked.failure.kind === 'unauthorized' && revoked.failure.error?.details.reason).toBe('revoked');
  });

  it('sends the Bearer token, sorts decks by path and asks note classifications of a deck', async () => {
    const { mock, api } = await paired();
    const decks = await api.decks();
    expect(decks.ok && decks.value.map((d) => d.path)).toEqual(['İngilizce', 'İngilizce::Fiiller', 'Türkçe']);
    expect(mock.log.at(-1)!.headers.Authorization).toMatch(/^Bearer tok-/);
    const cls = await api.classifications('d-en');
    expect(cls.ok && cls.value.map((c) => c.def.name)).toEqual(['Seviye']);
    expect(JSON.parse(mock.log.at(-1)!.body!)).toEqual({ deck_id: 'd-en', target: 'note' });
  });

  it('adds to the inbox without a `source` field (the server derives it) and dedupes by client_ref', async () => {
    const { mock, api } = await paired();
    const item = inboxItem({ front: 'badger', deck_id: 'd-en', attributes: { 'attr-seviye': 'opt-B1' }, context: { url: 'https://e.x', title: 'T', sentence: 'The badger dug.' }, client_ref: 'r1' });
    const r = await api.addToInbox(item);
    expect(r.ok).toBe(true);
    expect(Object.keys(JSON.parse(mock.log.at(-1)!.body!))).not.toContain('source');
    expect((await api.addToInbox(item)).ok).toBe(true);
    expect(mock.inbox).toHaveLength(1);
    expect(mock.inbox[0]!.item).toEqual(item);
  });

  it('classifies failures: validation is a rejection, 503/429 are transient', async () => {
    const { mock, api } = await paired();
    const missing = await api.addToInbox(inboxItem({ deck_id: 'gone', front: 'x' }));
    expect(!missing.ok && rejection(missing.failure)?.code).toBe('NOT_FOUND_DECK');
    mock.inboxDisabled = true;
    const off = await api.addToInbox(inboxItem({ front: 'x' }));
    expect(!off.ok && isTransient(off.failure)).toBe(true);
    expect(!off.ok && rejection(off.failure)).toBeUndefined();
    const limited = (async () => new Response(JSON.stringify({ code: 'CONFLICT_RATE_LIMITED', message_key: '', message: '', details: {}, retryable: true }), { status: 429, headers: { 'Retry-After': '7' } })) as typeof fetch;
    const r = await new LocalApi({ port: 1, token: 't', fetch: limited }).addToInbox(inboxItem({ front: 'x' }));
    expect(r).toEqual({ ok: false, failure: { kind: 'rate_limited', retryAfter: 7 } });
  });

  it('is refused by the server from a web page origin (only extension origins pass)', async () => {
    const { mock } = setup();
    const web = mock.fetcher({ origin: 'https://evil.example' });
    const r = await new LocalApi({ port: mock.port, fetch: web }).requestPairing('x');
    expect(!r.ok && r.failure.kind === 'api' && r.failure.error.code).toBe('PERMISSION_ORIGIN_NOT_ALLOWED');
  });
});
