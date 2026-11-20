/**
 * Items that could not be delivered yet (app closed, module off, token gone). Kept in
 * `chrome.storage.local`, sent in order once the app answers (docs/06 §5: "Uygulama kapalıysa istek
 * kuyruğa alınır, uygulama açılınca gönderilir"). Every item carries a `client_ref`, so a resend
 * after a lost response cannot create a duplicate inbox item (the API dedupes per client).
 */
import type { AddInboxItem, ApiError } from './contract';
import { rejection, type Failure, type Result } from './api';
import { KEYS, type Kv } from './kv';

/** One queued item. */
export type Queued = {
  /** = `item.client_ref`. */
  ref: string;
  item: AddInboxItem;
  queuedAt: number;
  attempts: number;
  /** `failed`: the app rejected it (e.g. deck deleted); waits for the user (retry/discard). */
  state: 'waiting' | 'failed';
  error?: Pick<ApiError, 'code' | 'message_key' | 'details'> | undefined;
};

/** Hard cap so a never-opened app cannot fill the browser's storage. */
export const MAX_QUEUE = 500;

export type FlushReport = {
  sent: number;
  failed: number;
  /** Why flushing stopped early (`undefined` = queue drained or only failed items left). */
  stoppedBy?: Failure | undefined;
};

/** Serializes every read-modify-write so concurrent flushes/submits never lose or resend items. */
export class Outbox {
  private chain: Promise<unknown> = Promise.resolve();

  constructor(private readonly kv: Kv) {}

  private exclusive<T>(fn: () => Promise<T>): Promise<T> {
    const run = this.chain.then(fn, fn);
    this.chain = run.catch(() => undefined);
    return run;
  }

  private async load(): Promise<Queued[]> {
    return (await this.kv.get<Queued[]>(KEYS.outbox)) ?? [];
  }

  private save(list: Queued[]): Promise<void> {
    return this.kv.set(KEYS.outbox, list);
  }

  /** Current contents (oldest first). */
  list(): Promise<Queued[]> {
    return this.exclusive(() => this.load());
  }

  /** Counts per state. */
  async counts(): Promise<{ waiting: number; failed: number }> {
    const list = await this.list();
    const failed = list.filter((q) => q.state === 'failed').length;
    return { waiting: list.length - failed, failed };
  }

  /** Appends an item (needs `client_ref`). Returns false when the queue is full. */
  enqueue(item: AddInboxItem, now: number): Promise<boolean> {
    return this.exclusive(async () => {
      const list = await this.load();
      const ref = item.client_ref ?? '';
      if (ref === '' || list.some((q) => q.ref === ref)) return ref !== '';
      if (list.length >= MAX_QUEUE) return false;
      list.push({ ref, item, queuedAt: now, attempts: 0, state: 'waiting' });
      await this.save(list);
      return true;
    });
  }

  /**
   * Sends waiting items oldest first with `send`. Delivered items leave the queue; a permanent
   * rejection marks the item `failed` and moves on; a transient problem (offline, rate limit, 503)
   * or a lost token stops the run and keeps everything for next time.
   */
  flush(send: (item: AddInboxItem) => Promise<Result<unknown>>): Promise<FlushReport> {
    return this.exclusive(async () => {
      const report: FlushReport = { sent: 0, failed: 0 };
      const list = await this.load();
      const keep: Queued[] = [];
      let stopped = false;
      for (const q of list) {
        if (stopped || q.state === 'failed') {
          keep.push(q);
          continue;
        }
        const r = await send(q.item);
        if (r.ok) {
          report.sent++;
          continue;
        }
        const error = rejection(r.failure);
        if (!error) {
          keep.push({ ...q, attempts: q.attempts + 1 });
          report.stoppedBy = r.failure;
          stopped = true;
          continue;
        }
        report.failed++;
        keep.push({
          ...q,
          attempts: q.attempts + 1,
          state: 'failed',
          error: { code: error.code, message_key: error.message_key, details: error.details },
        });
      }
      await this.save(keep);
      return report;
    });
  }

  /** Puts failed items back in line. */
  retryFailed(): Promise<number> {
    return this.exclusive(async () => {
      const list = await this.load();
      let n = 0;
      const next = list.map((q) => {
        if (q.state !== 'failed') return q;
        n++;
        return { ...q, state: 'waiting' as const, error: undefined };
      });
      await this.save(next);
      return n;
    });
  }

  /** Drops failed items. */
  discardFailed(): Promise<number> {
    return this.exclusive(async () => {
      const list = await this.load();
      const keep = list.filter((q) => q.state !== 'failed');
      await this.save(keep);
      return list.length - keep.length;
    });
  }
}
