/**
 * In-memory stand-in for the desktop app's local HTTP API, written from docs/06 §3 "Uygulanan
 * hâli" (track I). Used by the unit tests (as a `fetch`) and the e2e test (behind a real Node HTTP
 * server). It checks what the real server checks that matters to the extension: Host, Origin
 * (web origins refused, token bound to the pairing origin), the CSRF guard (`X-Flashcard-Client`
 * on every call but ping, `application/json` on every POST), Bearer token + scopes, unknown
 * fields in `AddInboxItem` (deny_unknown_fields), and `client_ref` dedupe.
 */
import type { AddInboxItem, DeckDto, EffectiveAttributeDto, InboxItemDto } from '../../src/lib/contract';

export type MockRequest = { method: string; path: string; headers: Record<string, string>; body?: string };
export type MockResponse = { status: number; headers: Record<string, string>; body: unknown };

type Client = { id: string; origin: string | null; scopes: string[]; token: string; status: 'pending' | 'approved' | 'denied' | 'revoked'; tokenGiven: boolean };

const ADD_INBOX_FIELDS = new Set(['front', 'back', 'fields', 'deck_id', 'notetype_id', 'tags', 'attributes', 'context', 'client_ref']);

export class MockApi {
  port = 28735;
  decks: DeckDto[] = [];
  classifications: Record<string, EffectiveAttributeDto[]> = {};
  inbox: (InboxItemDto & { client_id: string })[] = [];
  clients = new Map<string, Client>();
  /** Pairing requests are approved automatically after this many polls (-1 = never). */
  autoApproveAfterPolls = 1;
  /** Answer every request with 503 CONFLICT_MODULE_DISABLED. */
  inboxDisabled = false;
  log: MockRequest[] = [];
  private polls = new Map<string, number>();
  private seq = 0;

  private err(status: number, code: string, details: Record<string, unknown> = {}, extra: Record<string, string> = {}): MockResponse {
    return { status, headers: extra, body: { code, message_key: `errors.${code}`, message: code, details, retryable: status === 429 } };
  }

  approve(clientId: string): void {
    const c = this.clients.get(clientId);
    if (c) c.status = 'approved';
  }

  revokeAll(): void {
    for (const c of this.clients.values()) c.status = 'revoked';
  }

  handle(req: MockRequest): MockResponse {
    this.log.push(req);
    const h = Object.fromEntries(Object.entries(req.headers).map(([k, v]) => [k.toLowerCase(), v]));
    const host = h.host ?? `127.0.0.1:${this.port}`;
    if (![`127.0.0.1:${this.port}`, `localhost:${this.port}`, `[::1]:${this.port}`].includes(host)) return this.err(403, 'PERMISSION_ORIGIN_NOT_ALLOWED');
    const origin = h.origin ?? null;
    if (origin !== null && !/^(chrome-extension|moz-extension|extension):\/\//.test(origin)) return this.err(403, 'PERMISSION_ORIGIN_NOT_ALLOWED');
    const url = new URL(req.path, 'http://x');
    const path = url.pathname;
    let body: unknown;
    try {
      body = req.body ? JSON.parse(req.body) : undefined;
    } catch {
      return this.err(400, 'VALIDATION_INVALID_PARAMS');
    }
    if (req.method === 'GET' && path === '/v1/ping') return { status: 200, headers: {}, body: { app: 'flashcard', api_version: 'v1', version: '1.0.0' } };
    const clientHeader = (h['x-flashcard-client'] ?? '').trim();
    if (clientHeader.length === 0 || clientHeader.length > 100) return this.err(400, 'VALIDATION_INVALID_PARAMS', { reason: 'client_header' });
    if (req.method === 'POST' && (h['content-type'] ?? '').split(';')[0]?.trim().toLowerCase() !== 'application/json')
      return this.err(400, 'VALIDATION_INVALID_PARAMS', { reason: 'content_type' });
    if (req.method === 'POST' && path === '/v1/pair') {
      const b = body as { client_name?: string; client_kind?: string; scopes?: string[] };
      if (typeof b?.client_name !== 'string' || !Array.isArray(b.scopes)) return this.err(400, 'VALIDATION_INVALID_PARAMS');
      const id = `client-${++this.seq}`;
      this.clients.set(id, { id, origin, scopes: b.scopes, token: `tok-${id}`, status: 'pending', tokenGiven: false });
      return { status: 201, headers: {}, body: { request_id: id, status: 'pending', expires_at: Date.now() + 600_000 } };
    }
    const pairMatch = /^\/v1\/pair\/([^/]+)$/.exec(path);
    if (req.method === 'POST' && pairMatch) {
      const c = this.clients.get(decodeURIComponent(pairMatch[1] ?? ''));
      if (!c) return this.err(404, 'NOT_FOUND_PAIRING');
      // Same origin as the pairing request (Chromium sends it on an extension's POSTs).
      if (origin !== null && c.origin !== origin) return this.err(403, 'PERMISSION_ORIGIN_NOT_ALLOWED');
      const n = (this.polls.get(c.id) ?? 0) + 1;
      this.polls.set(c.id, n);
      if (c.status === 'pending' && this.autoApproveAfterPolls >= 0 && n >= this.autoApproveAfterPolls) c.status = 'approved';
      if (c.status === 'approved') {
        const first = !c.tokenGiven;
        c.tokenGiven = true;
        return { status: 200, headers: {}, body: { status: 'approved', client_id: c.id, scopes: c.scopes, ...(first ? { token: c.token } : {}) } };
      }
      return { status: 200, headers: {}, body: { status: c.status === 'revoked' ? 'denied' : c.status, scopes: c.scopes } };
    }
    // Everything below needs a token.
    const token = /^Bearer (.+)$/.exec(h.authorization ?? '')?.[1];
    const client = [...this.clients.values()].find((c) => c.token === token);
    if (!client) return this.err(401, 'PERMISSION_UNAUTHENTICATED', { reason: token ? 'invalid' : 'missing' });
    if (client.status !== 'approved') return this.err(401, 'PERMISSION_UNAUTHENTICATED', { reason: client.status });
    if (origin !== null && client.origin !== origin) return this.err(403, 'PERMISSION_ORIGIN_NOT_ALLOWED');
    const need = (scope: string): MockResponse | undefined => (client.scopes.includes(scope) ? undefined : this.err(403, 'PERMISSION_DENIED', { scope }));
    if (req.method === 'GET' && path === '/v1/auth') return { status: 200, headers: {}, body: { id: client.id, name: 'x', kind: 'browser_extension', scopes: client.scopes, status: client.status } };
    if (req.method === 'GET' && path === '/v1/decks') return need('decks:read') ?? { status: 200, headers: {}, body: { decks: this.decks } };
    if (req.method === 'POST' && path === '/v1/attributes.effective') {
      const b = body as { deck_id?: string; target?: string };
      if (typeof b?.deck_id !== 'string' || b.target !== 'note') return this.err(400, 'VALIDATION_INVALID_PARAMS');
      if (!this.decks.some((d) => d.id === b.deck_id)) return this.err(404, 'NOT_FOUND_DECK');
      return need('attributes:read') ?? { status: 200, headers: {}, body: { attributes: this.classifications[b.deck_id] ?? [] } };
    }
    if (req.method === 'POST' && path === '/v1/inbox') {
      const denied = need('inbox:write');
      if (denied) return denied;
      if (this.inboxDisabled) return this.err(503, 'CONFLICT_MODULE_DISABLED');
      const b = body as AddInboxItem & Record<string, unknown>;
      if (typeof b !== 'object' || b === null) return this.err(400, 'VALIDATION_INVALID_PARAMS');
      const unknown = Object.keys(b).filter((k) => !ADD_INBOX_FIELDS.has(k));
      if (unknown.length > 0) return this.err(400, 'VALIDATION_INVALID_PARAMS', { unknown });
      if (!b.front && !b.back && Object.keys(b.fields ?? {}).length === 0) return this.err(400, 'VALIDATION_FIELD_REQUIRED');
      if (b.deck_id && !this.decks.some((d) => d.id === b.deck_id)) return this.err(404, 'NOT_FOUND_DECK');
      if (b.client_ref) {
        const existing = this.inbox.find((i) => i.client_id === client.id && i.item.client_ref === b.client_ref);
        if (existing) return { status: 200, headers: {}, body: existing };
      }
      const now = Date.now();
      const dto = {
        id: `inbox-${++this.seq}`,
        source: { kind: 'local_api' as const, client_id: client.id, client_name: 'x' },
        status: 'pending' as const,
        item: b,
        note_ids: [],
        created_at: now,
        updated_at: now,
        client_id: client.id,
      };
      this.inbox.push(dto);
      return { status: 201, headers: {}, body: dto };
    }
    return this.err(404, 'NOT_FOUND_METHOD');
  }

  /** A `fetch` that talks to this mock (unit tests). `online=false` simulates a closed app. */
  fetcher(opts: { origin?: string; online?: () => boolean } = {}): typeof fetch {
    return (async (input: RequestInfo | URL, init?: RequestInit) => {
      if (opts.online && !opts.online()) throw new TypeError('Failed to fetch');
      const url = new URL(String(input));
      const headers: Record<string, string> = { ...(init?.headers as Record<string, string>), host: url.host };
      if (opts.origin) headers.origin = opts.origin;
      const res = this.handle({ method: init?.method ?? 'GET', path: url.pathname + url.search, headers, body: (init?.body as string | null) ?? undefined });
      return new Response(JSON.stringify(res.body), { status: res.status, headers: { 'content-type': 'application/json', ...res.headers } });
    }) as typeof fetch;
  }
}

/** Sample data: an "İngilizce" deck with a CEFR "Seviye" classification (docs/02 §8.1). */
export function sampleData(api: MockApi): void {
  const now = 1_700_000_000_000;
  const deck = (id: string, name: string, path: string, parent: string | null): DeckDto => ({
    id, parent_id: parent, name, path, description: '', icon: null, color: null, preset_id: null, position: 0, created_at: now, updated_at: now,
  });
  api.decks = [deck('d-tr', 'Türkçe', 'Türkçe', null), deck('d-en', 'İngilizce', 'İngilizce', null), deck('d-en-v', 'Fiiller', 'İngilizce::Fiiller', 'd-en')];
  const levels = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'].map((label, order) => ({ id: `opt-${label}`, label, color: null, order }));
  const seviye: EffectiveAttributeDto = {
    def: { id: 'attr-seviye', scope_deck_id: 'd-en', key: 'seviye', name: 'Seviye', kind: 'ordered_enum', options: levels, target: 'note', required: false, default_value: null, position: 0, template_id: 'cefr', updated_at: now },
    inherited: false,
    suggested_value: null,
  };
  api.classifications = { 'd-en': [seviye], 'd-en-v': [{ ...seviye, inherited: true }] };
}
