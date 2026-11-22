import { afterEach, describe, expect, it, vi } from 'vitest';
import { createBridgeTransport } from './bridge';
import { createApiClient } from './client';
import type { EventEnvelope } from './types';

class FakeEventSource {
  static all: FakeEventSource[] = [];
  onmessage: ((m: MessageEvent<string>) => void) | null = null;
  closed = false;
  constructor(readonly url: string) {
    FakeEventSource.all.push(this);
  }
  close() {
    this.closed = true;
  }
  push(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) } as MessageEvent<string>);
  }
}

function reply(body: unknown, status = 200) {
  return { status, json: async () => body } as Response;
}

afterEach(() => {
  vi.unstubAllGlobals();
  FakeEventSource.all = [];
});

describe('dev bridge transport', () => {
  it('posts {method, params} with the token and unwraps the result', async () => {
    const fetchMock = vi.fn(async () => reply({ ok: true, result: { id: 'd1' } }));
    vi.stubGlobal('fetch', fetchMock);
    const api = createApiClient(createBridgeTransport('http://127.0.0.1:4000/', 'tok'));
    await expect(api.deck.create('Fiiller')).resolves.toEqual({ id: 'd1' });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('http://127.0.0.1:4000/rpc');
    expect((init.headers as Record<string, string>).authorization).toBe('Bearer tok');
    expect(JSON.parse(init.body as string)).toEqual({
      method: 'v1.decks.create',
      params: { path: 'Fiiller', parent_id: null },
    });
  });

  it('turns the error envelope into the same ApiError an IPC rejection gives', async () => {
    const error = { code: 'VALIDATION_INVALID_PARAMS', message_key: 'errors.validation.invalid_params', message: 'x', details: { f: 1 }, retryable: false };
    vi.stubGlobal('fetch', vi.fn(async () => reply({ ok: false, error }, 200)));
    const api = createApiClient(createBridgeTransport('http://127.0.0.1:4000', 'tok'));
    await expect(api.deck.create('')).rejects.toMatchObject({ code: 'VALIDATION_INVALID_PARAMS', details: { f: 1 } });
    vi.stubGlobal('fetch', vi.fn(async () => reply(null, 502)));
    await expect(api.deck.create('a')).rejects.toMatchObject({ code: 'INTERNAL' });
  });

  it('shares one event stream between listeners and closes it after the last one', async () => {
    vi.stubGlobal('EventSource', FakeEventSource);
    const t = createBridgeTransport('http://127.0.0.1:4000', 'a b');
    const a: EventEnvelope[] = [];
    const b: EventEnvelope[] = [];
    const offA = await t.listen((e) => a.push(e));
    const offB = await t.listen((e) => b.push(e));
    expect(FakeEventSource.all).toHaveLength(1);
    const es = FakeEventSource.all[0]!;
    expect(es.url).toBe('http://127.0.0.1:4000/events?token=a%20b');
    const env = { seq: 1, at: 0, batch_id: null, event: { type: 'data_changed', data: {} } };
    es.push(env);
    expect(a).toEqual([env]);
    expect(b).toEqual([env]);
    offA();
    expect(es.closed).toBe(false);
    offB();
    expect(es.closed).toBe(true);
  });
});
