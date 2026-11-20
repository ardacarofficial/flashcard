/**
 * Client for the desktop app's local HTTP API (docs/06 §3). The extension is an ordinary client:
 * Bearer token, only `127.0.0.1`, only the scopes it was granted. Every call resolves to a
 * `Result` — network trouble is a value (`offline`), not an exception, so callers can queue.
 */
import { PATHS, SCOPES } from './contract';
import type {
  ApiError,
  DeckDto,
  DeckList,
  EffectiveAttributeDto,
  EffectiveAttributeList,
  EffectiveAttributes,
  AddInboxItem,
  ApiClientDto,
  InboxItemDto,
  PairRequest,
  PairResponse,
  PairStatusResponse,
  PingResponse,
} from './contract';
import { APP_ID, CLIENT_HEADER, CLIENT_ID } from './contract';

/** Why a call failed. */
export type Failure =
  /** App not running / port closed / timed out. */
  | { kind: 'offline' }
  /** Token missing, revoked, denied or unknown (401): pair again. */
  | { kind: 'unauthorized'; error?: ApiError | undefined }
  /** Too many requests (429); retry after `retryAfter` seconds. */
  | { kind: 'rate_limited'; retryAfter: number }
  /** The app answered with an error (validation, permission, not found…). */
  | { kind: 'api'; status: number; error: ApiError }
  /** Something that is not our API answered. */
  | { kind: 'protocol'; status: number };

export type Result<T> = { ok: true; value: T } | { ok: false; failure: Failure };

export type ApiConfig = {
  port: number;
  token?: string | undefined;
  /** Per-request timeout (ms). */
  timeoutMs?: number;
  fetch?: typeof fetch;
};

/** True when a failure is worth retrying later without user action. */
export function isTransient(f: Failure): boolean {
  if (f.kind === 'offline' || f.kind === 'rate_limited') return true;
  // 503: the app is up but the inbox module is switched off; keep the item for later.
  return f.kind === 'api' && (f.error.retryable || f.status === 503 || f.status >= 500);
}

/**
 * The app's error when the failure is a permanent rejection (fix the item, don't retry);
 * `undefined` for anything worth retrying or needing re-pairing.
 */
export function rejection(f: Failure): ApiError | undefined {
  return f.kind === 'api' && !isTransient(f) ? f.error : undefined;
}

function isApiError(v: unknown): v is ApiError {
  return typeof v === 'object' && v !== null && typeof (v as ApiError).code === 'string';
}

/** Thin, typed HTTP client. */
export class LocalApi {
  constructor(private readonly cfg: ApiConfig) {}

  /** The token this client sends (so a 401 can be attributed to it). */
  get token(): string | undefined {
    return this.cfg.token;
  }

  get baseUrl(): string {
    return `http://127.0.0.1:${this.cfg.port}`;
  }

  private async request<T>(method: 'GET' | 'POST', path: string, body?: unknown, auth = true): Promise<Result<T>> {
    const headers: Record<string, string> = { Accept: 'application/json' };
    // Track I's CSRF guard: the client header on every call but ping, JSON on every POST.
    if (path !== PATHS.ping) headers[CLIENT_HEADER] = CLIENT_ID;
    if (method === 'POST') headers['Content-Type'] = 'application/json';
    if (auth) {
      if (!this.cfg.token) return { ok: false, failure: { kind: 'unauthorized' } };
      headers.Authorization = `Bearer ${this.cfg.token}`;
    }
    const doFetch = this.cfg.fetch ?? fetch;
    let res: Response;
    try {
      res = await doFetch(this.baseUrl + path, {
        method,
        headers,
        body: body === undefined ? null : JSON.stringify(body),
        signal: AbortSignal.timeout(this.cfg.timeoutMs ?? 5000),
        credentials: 'omit',
        cache: 'no-store',
      });
    } catch {
      return { ok: false, failure: { kind: 'offline' } };
    }
    let json: unknown = undefined;
    try {
      json = await res.json();
    } catch {
      json = undefined;
    }
    if (res.ok) {
      return json === undefined ? { ok: false, failure: { kind: 'protocol', status: res.status } } : { ok: true, value: json as T };
    }
    const error = isApiError(json) ? json : isApiError((json as { error?: unknown } | undefined)?.error) ? (json as { error: ApiError }).error : undefined;
    if (res.status === 401) return { ok: false, failure: { kind: 'unauthorized', error } };
    if (res.status === 429) {
      const after = Number(res.headers.get('Retry-After'));
      return { ok: false, failure: { kind: 'rate_limited', retryAfter: Number.isFinite(after) && after > 0 ? after : 1 } };
    }
    if (error) return { ok: false, failure: { kind: 'api', status: res.status, error } };
    return { ok: false, failure: { kind: 'protocol', status: res.status } };
  }

  /** Is the app running (no token needed)? Something else on the port counts as offline. */
  async ping(): Promise<Result<PingResponse>> {
    const r = await this.request<PingResponse>('GET', PATHS.ping, undefined, false);
    if (r.ok && r.value.app !== APP_ID) return { ok: false, failure: { kind: 'protocol', status: 200 } };
    return r;
  }

  /** The extension's own client record: is the token still valid? */
  auth(): Promise<Result<ApiClientDto>> {
    return this.request('GET', PATHS.auth);
  }

  /** Asks the user (in the app) to approve this extension. */
  requestPairing(clientName: string): Promise<Result<PairResponse>> {
    const body: PairRequest = { client_name: clientName, client_kind: 'browser_extension', scopes: [...SCOPES] };
    return this.request('POST', PATHS.pair, body, false);
  }

  /** Pairing state; carries the token once approved. A POST, so Chromium sends the Origin the
   * token is bound to. */
  pairingStatus(requestId: string): Promise<Result<PairStatusResponse>> {
    return this.request('POST', PATHS.pairStatus(requestId), {}, false);
  }

  /** All decks (sorted by path). */
  async decks(): Promise<Result<DeckDto[]>> {
    const r = await this.request<DeckList>('GET', PATHS.decks);
    if (!r.ok) return r;
    const decks = [...r.value.decks].sort((a, b) => a.path.localeCompare(b.path));
    return { ok: true, value: decks };
  }

  /** Classifications that apply to notes in `deckId` (own + inherited). */
  async classifications(deckId: string): Promise<Result<EffectiveAttributeDto[]>> {
    const body: EffectiveAttributes = { deck_id: deckId, target: 'note' };
    const r = await this.request<EffectiveAttributeList>('POST', PATHS.method('attributes.effective'), body);
    return r.ok ? { ok: true, value: r.value.attributes } : r;
  }

  /** Drops an item into the app's inbox. */
  addToInbox(item: AddInboxItem): Promise<Result<InboxItemDto>> {
    return this.request('POST', PATHS.inbox, item);
  }
}
