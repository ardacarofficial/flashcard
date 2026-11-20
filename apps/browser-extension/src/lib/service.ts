/**
 * Everything the service worker does, without `chrome.*` (injected), so it is unit-testable.
 * The popup and options page never talk to the app themselves: they send messages here, so there
 * is one owner of the token, the outbox and the pairing poll.
 */
import type {
  AddInboxItem,
  ApiError,
  DeckDto,
  EffectiveAttributeDto,
  PairStatusResponse,
} from './contract';
import { DEFAULT_PORT } from './contract';
import { LocalApi, rejection, type Failure, type Result } from './api';
import { Outbox, type FlushReport, type Queued } from './outbox';
import { KEYS, type Kv } from './kv';

/** User settings (options page). */
export type Settings = { port: number };

/** Pairing as stored. The token never leaves the service worker except in requests to the app. */
export type Pairing = {
  token?: string | undefined;
  clientId?: string | undefined;
  scopes?: string[] | undefined;
  pending?: { requestId: string; expiresAt: number } | undefined;
  /** Outcome of the last attempt / why the token was dropped (shown once). */
  last?: 'denied' | 'expired' | 'revoked' | undefined;
};

/** What the popup/options page shows. */
export type Status = {
  app: 'online' | 'offline';
  paired: boolean;
  pairing: 'none' | 'pending' | 'paired';
  last?: Pairing['last'];
  port: number;
  outbox: { waiting: number; failed: number };
};

/** Last decks/classifications seen, so the popup works (and queues) while the app is closed. */
type Cache = { decks?: DeckDto[]; classifications?: Record<string, EffectiveAttributeDto[]> };

/** Result of an add from the popup. */
export type SubmitResult =
  | { status: 'sent' }
  /** Kept for later: app closed / module off / not paired. */
  | { status: 'queued'; reason: 'offline' | 'unpaired' | 'busy' }
  /** The app rejected it; nothing was queued, the user can fix the form. */
  | { status: 'rejected'; error: ApiError }
  | { status: 'full' };

export type Deps = {
  local: Kv;
  fetch?: typeof fetch;
  now: () => number;
  uuid: () => string;
  /** Name shown in the app's approval dialog. */
  clientName: string;
};

export class Service {
  readonly outbox: Outbox;

  constructor(private readonly deps: Deps) {
    this.outbox = new Outbox(deps.local);
  }

  async settings(): Promise<Settings> {
    const s = await this.deps.local.get<Partial<Settings>>(KEYS.settings);
    const port = s?.port;
    return { port: typeof port === 'number' && port > 0 && port < 65536 ? port : DEFAULT_PORT };
  }

  async setPort(port: number): Promise<Settings> {
    if (!Number.isInteger(port) || port <= 0 || port >= 65536) return this.settings();
    await this.deps.local.set(KEYS.settings, { port });
    return { port };
  }

  private async pairing(): Promise<Pairing> {
    return (await this.deps.local.get<Pairing>(KEYS.pairing)) ?? {};
  }

  private savePairing(p: Pairing): Promise<void> {
    return this.deps.local.set(KEYS.pairing, p);
  }

  private async api(): Promise<LocalApi> {
    const [{ port }, p] = await Promise.all([this.settings(), this.pairing()]);
    return new LocalApi({ port, token: p.token, fetch: this.deps.fetch });
  }

  /** On 401 the token is useless: forget it (the queue stays) so the popup offers pairing again. */
  private async onFailure(f: Failure): Promise<void> {
    if (f.kind !== 'unauthorized') return;
    const p = await this.pairing();
    if (!p.token) return;
    await this.savePairing({ last: 'revoked' });
  }

  async status(): Promise<Status> {
    const api = await this.api();
    const [ping, p, outbox, { port }] = await Promise.all([api.ping(), this.pairing(), this.outbox.counts(), this.settings()]);
    const pending = p.pending && p.pending.expiresAt > this.deps.now() ? p.pending : undefined;
    return {
      app: ping.ok ? 'online' : 'offline',
      paired: !!p.token,
      pairing: p.token ? 'paired' : pending ? 'pending' : 'none',
      last: p.last,
      port,
      outbox,
    };
  }

  /** Starts pairing: the app shows an approval dialog. */
  async startPairing(): Promise<Result<Pairing>> {
    const api = await this.api();
    const r = await api.requestPairing(this.deps.clientName);
    if (!r.ok) return r;
    const p: Pairing = { pending: { requestId: r.value.request_id, expiresAt: r.value.expires_at } };
    await this.savePairing(p);
    return { ok: true, value: p };
  }

  /**
   * Polls the pending request once. The token is only in the **first** approved answer, so it is
   * stored before anything else happens.
   */
  async pollPairing(): Promise<Result<Status['pairing'] | 'denied' | 'expired'>> {
    const p = await this.pairing();
    if (p.token) return { ok: true, value: 'paired' };
    if (!p.pending) return { ok: true, value: 'none' };
    const api = await this.api();
    const r = await api.pairingStatus(p.pending.requestId);
    if (!r.ok) {
      if (r.failure.kind === 'api' && r.failure.status === 404) {
        await this.savePairing({ last: 'expired' });
        return { ok: true, value: 'expired' };
      }
      return r;
    }
    return { ok: true, value: await this.applyPairStatus(r.value) };
  }

  private async applyPairStatus(s: PairStatusResponse): Promise<Status['pairing'] | 'denied' | 'expired'> {
    switch (s.status) {
      case 'approved':
        if (!s.token) {
          // Approved, but the token went to an earlier poll that never got stored: start over.
          await this.savePairing({ last: 'expired' });
          return 'expired';
        }
        await this.savePairing({ token: s.token, clientId: s.client_id ?? undefined, scopes: s.scopes });
        return 'paired';
      case 'denied':
        await this.savePairing({ last: 'denied' });
        return 'denied';
      case 'expired':
        await this.savePairing({ last: 'expired' });
        return 'expired';
      case 'pending':
        return 'pending';
    }
  }

  /** Forgets the token (the app keeps the client until revoked there). */
  async forgetPairing(): Promise<void> {
    await this.savePairing({});
  }

  private async cache(): Promise<Cache> {
    return (await this.deps.local.get<Cache>(KEYS.cache)) ?? {};
  }

  /** Decks from the app, or the last known list when it is closed. */
  async decks(): Promise<{ decks: DeckDto[]; cached: boolean }> {
    const api = await this.api();
    const r = await api.decks();
    const cache = await this.cache();
    if (r.ok) {
      await this.deps.local.set(KEYS.cache, { ...cache, decks: r.value });
      return { decks: r.value, cached: false };
    }
    await this.onFailure(r.failure);
    return { decks: cache.decks ?? [], cached: true };
  }

  /** Classifications of a deck, or the last known ones. */
  async classifications(deckId: string): Promise<{ list: EffectiveAttributeDto[]; cached: boolean }> {
    const api = await this.api();
    const r = await api.classifications(deckId);
    const cache = await this.cache();
    if (r.ok) {
      const classifications = { ...(cache.classifications ?? {}), [deckId]: r.value };
      await this.deps.local.set(KEYS.cache, { ...cache, classifications });
      return { list: r.value, cached: false };
    }
    await this.onFailure(r.failure);
    return { list: cache.classifications?.[deckId] ?? [], cached: true };
  }

  /**
   * Adds an item from the popup. Older queued items go first (order is kept); then this one is
   * sent. If the app cannot take it now, it is queued; if the app rejects it, the user is told.
   */
  async submit(input: Omit<AddInboxItem, 'client_ref'>): Promise<SubmitResult> {
    const item: AddInboxItem = { ...input, client_ref: this.deps.uuid() };
    const { waiting } = await this.outbox.counts();
    if (waiting > 0) {
      const before = await this.flush();
      if (before.stoppedBy) return this.queue(item, before.stoppedBy);
    }
    const api = await this.api();
    const r = await api.addToInbox(item);
    if (r.ok) return { status: 'sent' };
    await this.onFailure(r.failure);
    const error = rejection(r.failure);
    return error ? { status: 'rejected', error } : this.queue(item, r.failure);
  }

  private async queue(item: AddInboxItem, why: Failure): Promise<SubmitResult> {
    if (!(await this.outbox.enqueue(item, this.deps.now()))) return { status: 'full' };
    const reason = why.kind === 'unauthorized' ? 'unpaired' : why.kind === 'offline' || why.kind === 'protocol' ? 'offline' : 'busy';
    return { status: 'queued', reason };
  }

  /** Sends what is queued (alarm, browser start, popup open, options "send now"). */
  async flush(): Promise<FlushReport> {
    const p = await this.pairing();
    if (!p.token) return { sent: 0, failed: 0, stoppedBy: { kind: 'unauthorized' } };
    const api = await this.api();
    const report = await this.outbox.flush((item) => api.addToInbox(item));
    if (report.stoppedBy) await this.onFailure(report.stoppedBy);
    return report;
  }

  outboxItems(): Promise<Queued[]> {
    return this.outbox.list();
  }
}
