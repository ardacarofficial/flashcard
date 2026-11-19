import { describe, expect, it, vi } from 'vitest';
import { ApiError, createApiClient, toApiError } from './client';
import type { Transport } from './transport';
import type { FcEvent } from './types';
import { createMockTransport } from './mock/transport';

function fakeTransport(impl: Transport['invoke']) {
  const listeners: ((e: FcEvent) => void)[] = [];
  const t: Transport = {
    invoke: vi.fn(impl),
    listen: async (h) => {
      listeners.push(h);
      return () => listeners.splice(listeners.indexOf(h), 1);
    },
  };
  return { t, emit: (e: FcEvent) => listeners.forEach((l) => l(e)) };
}

describe('api client', () => {
  it('sends versioned command name and payload through the single invoke', async () => {
    const { t } = fakeTransport(async () => ({ id: 'd1' }));
    const api = createApiClient(t);
    await expect(api.deck.create('Fiiller', 'p1')).resolves.toEqual({ id: 'd1' });
    expect(t.invoke).toHaveBeenCalledWith('v1.deck.create', { name: 'Fiiller', parent_id: 'p1' });
  });

  it('builds a classic note payload with attributes', async () => {
    const { t } = fakeTransport(async () => ({ id: 'n1' }));
    const api = createApiClient(t);
    await api.note.createClassic('d1', 'go', 'gitmek', { lvl: 'opt-A1' });
    expect(t.invoke).toHaveBeenCalledWith('v1.note.create', {
      deck_id: 'd1',
      notetype_id: 'classic',
      fields: { front: 'go', back: 'gitmek' },
      attributes: { lvl: 'opt-A1' },
    });
  });

  it('turns backend error DTOs into ApiError', async () => {
    const dto = {
      code: 'NOTE_DUPLICATE',
      message_key: 'errors.note.duplicate',
      message: 'dup',
      details: { existing_note_id: 'x' },
      retryable: false,
    };
    const { t } = fakeTransport(async () => {
      throw dto;
    });
    const err = await createApiClient(t).note.list('d').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({ code: 'NOTE_DUPLICATE', messageKey: 'errors.note.duplicate', details: dto.details });
  });

  it('maps unknown failures to INTERNAL', () => {
    expect(toApiError(new Error('boom'))).toMatchObject({ code: 'INTERNAL', messageKey: 'errors.internal' });
    expect(toApiError('{"code":"X","message_key":"k","message":"","retryable":true}')).toMatchObject({
      code: 'X',
      retryable: true,
    });
  });

  it('filters events by prefix and unsubscribes', async () => {
    const { t, emit } = fakeTransport(async () => null);
    const api = createApiClient(t);
    const seen: string[] = [];
    const unsub = await api.on('deck.', (e) => seen.push(e.type));
    emit({ type: 'deck.created', payload: {} });
    emit({ type: 'note.created', payload: {} });
    unsub();
    emit({ type: 'deck.renamed', payload: {} });
    expect(seen).toEqual(['deck.created']);
  });
});

describe('mock transport (dev only)', () => {
  it('serves inherited classifications and emits events', async () => {
    const api = createApiClient(createMockTransport());
    const events: string[] = [];
    await api.on('', (e) => events.push(e.type));
    const [english] = await api.deck.tree();
    const sub = english!.children[0]!;
    const defs = await api.attribute.effective(sub.id);
    expect(defs.map((d) => d.name)).toEqual(['Seviye']);
    await api.note.createClassic(sub.id, 'go', 'gitmek', { [defs[0]!.id]: 'opt-B1' });
    expect(await api.note.list(sub.id)).toHaveLength(1);
    expect(events).toContain('note.created');
  });

  it('unknown commands fail with NOT_IMPLEMENTED', async () => {
    const t = createMockTransport();
    await expect(t.invoke('v1.nope', {})).rejects.toMatchObject({ code: 'NOT_IMPLEMENTED' });
  });
});
