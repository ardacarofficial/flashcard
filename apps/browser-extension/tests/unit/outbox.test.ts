import { describe, expect, it } from 'vitest';
import type { Result } from '../../src/lib/api';
import type { AddInboxItem, ApiError } from '../../src/lib/contract';
import { MemoryKv } from '../../src/lib/kv';
import { MAX_QUEUE, Outbox } from '../../src/lib/outbox';

const item = (ref: string): AddInboxItem => ({ front: ref, client_ref: ref });
const apiError = (code: string): ApiError => ({ code: code as ApiError['code'], message_key: '', message: '', details: {}, retryable: false });

describe('Outbox', () => {
  it('sends in order and empties', async () => {
    const box = new Outbox(new MemoryKv());
    for (const r of ['a', 'b', 'c']) await box.enqueue(item(r), 0);
    const seen: string[] = [];
    const rep = await box.flush(async (i) => {
      seen.push(i.client_ref!);
      return { ok: true, value: {} };
    });
    expect(seen).toEqual(['a', 'b', 'c']);
    expect(rep).toEqual({ sent: 3, failed: 0 });
    expect(await box.counts()).toEqual({ waiting: 0, failed: 0 });
  });

  it('stops at the first transient failure and keeps the rest in order', async () => {
    const box = new Outbox(new MemoryKv());
    for (const r of ['a', 'b', 'c']) await box.enqueue(item(r), 0);
    let n = 0;
    const rep = await box.flush(async (): Promise<Result<unknown>> => (n++ === 0 ? { ok: true, value: {} } : { ok: false, failure: { kind: 'offline' } }));
    expect(rep.sent).toBe(1);
    expect(rep.stoppedBy).toEqual({ kind: 'offline' });
    expect((await box.list()).map((q) => [q.ref, q.attempts])).toEqual([['b', 1], ['c', 0]]);
  });

  it('marks permanent rejections failed, moves on, and lets the user retry or discard', async () => {
    const box = new Outbox(new MemoryKv());
    for (const r of ['bad', 'good']) await box.enqueue(item(r), 0);
    const rep = await box.flush(async (i): Promise<Result<unknown>> =>
      i.client_ref === 'bad' ? { ok: false, failure: { kind: 'api', status: 404, error: apiError('NOT_FOUND_DECK') } } : { ok: true, value: {} },
    );
    expect(rep).toEqual({ sent: 1, failed: 1 });
    const [failed] = await box.list();
    expect(failed).toMatchObject({ ref: 'bad', state: 'failed', error: { code: 'NOT_FOUND_DECK' } });
    // failed items are skipped by later flushes
    expect((await box.flush(async () => ({ ok: true, value: {} }))).sent).toBe(0);
    expect(await box.retryFailed()).toBe(1);
    expect(await box.counts()).toEqual({ waiting: 1, failed: 0 });
    await box.flush(async () => ({ ok: false, failure: { kind: 'api', status: 400, error: apiError('VALIDATION_FIELD_REQUIRED') } }));
    expect(await box.discardFailed()).toBe(1);
    expect(await box.list()).toEqual([]);
  });

  it('ignores duplicates, refuses items without client_ref, and caps its size', async () => {
    const box = new Outbox(new MemoryKv());
    expect(await box.enqueue({ front: 'x' }, 0)).toBe(false);
    expect(await box.enqueue(item('a'), 0)).toBe(true);
    expect(await box.enqueue(item('a'), 0)).toBe(true);
    expect((await box.list()).length).toBe(1);
    for (let i = 1; i < MAX_QUEUE; i++) await box.enqueue(item(`i${i}`), 0);
    expect(await box.enqueue(item('overflow'), 0)).toBe(false);
  });

  it('never sends an item twice when flushes overlap', async () => {
    const box = new Outbox(new MemoryKv());
    for (const r of ['a', 'b']) await box.enqueue(item(r), 0);
    const sent: string[] = [];
    const send = async (i: AddInboxItem): Promise<Result<unknown>> => {
      await new Promise((r) => setTimeout(r, 5));
      sent.push(i.client_ref!);
      return { ok: true, value: {} };
    };
    await Promise.all([box.flush(send), box.flush(send), box.enqueue(item('c'), 0), box.flush(send)]);
    expect(sent.sort()).toEqual(['a', 'b', 'c']);
  });
});
