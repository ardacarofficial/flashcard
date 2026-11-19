import { describe, expect, it, vi } from 'vitest';
import { ApiError, CLASSIC_NOTETYPE_ID, createApiClient, toApiError } from './client';
import type { Transport } from './transport';
import type { DomainEvent, EventEnvelope } from './types';
import { createMockTransport } from './mock/transport';

function fakeTransport(impl: Transport['invoke']) {
  const listeners: ((e: EventEnvelope) => void)[] = [];
  const t: Transport = {
    invoke: vi.fn(impl),
    listen: async (h) => {
      listeners.push(h);
      return () => listeners.splice(listeners.indexOf(h), 1);
    },
  };
  let seq = 0;
  const emit = (event: DomainEvent) =>
    listeners.forEach((l) => l({ seq: ++seq, at: 0, batch_id: null, event }));
  return { t, emit };
}

describe('api client', () => {
  it('sends the qualified method name and typed params through the single invoke', async () => {
    const { t } = fakeTransport(async () => ({ id: 'd1' }));
    const api = createApiClient(t);
    await expect(api.deck.create('Dil::Fiiller')).resolves.toEqual({ id: 'd1' });
    expect(t.invoke).toHaveBeenCalledWith('v1.decks.create', { path: 'Dil::Fiiller' });
    await api.deck.remove('d1');
    expect(t.invoke).toHaveBeenLastCalledWith('v1.decks.delete', { id: 'd1' });
    await api.attribute.effective('d1');
    expect(t.invoke).toHaveBeenLastCalledWith('v1.attributes.effective', { deck_id: 'd1', target: 'note' });
  });

  it('builds a classic note payload with attribute assignments', async () => {
    const { t } = fakeTransport(async () => ({ id: 'n1' }));
    const api = createApiClient(t);
    await api.note.createClassic('d1', 'go', 'gitmek', [
      { attr_id: 'lvl', value: { type: 'choice', value: 'opt-A1' } },
    ]);
    expect(t.invoke).toHaveBeenCalledWith('v1.notes.create', {
      notetype_id: CLASSIC_NOTETYPE_ID,
      deck_id: 'd1',
      fields: { front: 'go', back: 'gitmek' },
      tags: [],
      attributes: [{ attr_id: 'lvl', value: { type: 'choice', value: 'opt-A1' } }],
    });
  });

  it('turns backend error DTOs into ApiError', async () => {
    const dto = {
      code: 'CONFLICT_NOTE_DUPLICATE',
      message_key: 'errors.conflict.note_duplicate',
      message: 'dup',
      details: { existing_note_id: 'x' },
      retryable: false,
    };
    const { t } = fakeTransport(async () => {
      throw dto;
    });
    const err = await createApiClient(t).note.list('d').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({
      code: 'CONFLICT_NOTE_DUPLICATE',
      messageKey: 'errors.conflict.note_duplicate',
      details: dto.details,
    });
  });

  it('maps unknown failures to INTERNAL', () => {
    expect(toApiError(new Error('boom'))).toMatchObject({
      code: 'INTERNAL',
      messageKey: 'errors.internal.unexpected',
    });
    expect(toApiError('{"code":"IO_DATABASE","message_key":"k","message":"","retryable":true}')).toMatchObject({
      code: 'IO_DATABASE',
      retryable: true,
    });
  });

  it('filters events by type prefix and unsubscribes', async () => {
    const { t, emit } = fakeTransport(async () => null);
    const api = createApiClient(t);
    const seen: string[] = [];
    const unsub = await api.on(['deck_', 'undone'], (e) => seen.push(e.event.type));
    emit({ type: 'deck_deleted', data: { id: 'd', affected: { decks: 1, notes: 0, cards: 0 } } });
    emit({ type: 'notes_deleted', data: { ids: [] } });
    emit({ type: 'undone', data: { reverted_batch_id: 'b', summary: { key: 'k', params: {} } } });
    unsub();
    emit({ type: 'deck_deleted', data: { id: 'e', affected: { decks: 1, notes: 0, cards: 0 } } });
    expect(seen).toEqual(['deck_deleted', 'undone']);
  });
});

describe('mock transport (dev only)', () => {
  it('serves inherited classifications and emits event envelopes', async () => {
    const api = createApiClient(createMockTransport());
    const events: string[] = [];
    await api.on('', (e) => events.push(e.event.type));
    const { roots } = await api.deck.tree();
    const sub = roots[0]!.children[0]!;
    expect(sub.deck.path).toBe('İngilizce::Fiiller');
    const { attributes } = await api.attribute.effective(sub.deck.id);
    expect(attributes.map((a) => [a.def.name, a.inherited])).toEqual([['Seviye', true]]);
    const b1 = attributes[0]!.def.options[2]!.id;
    const note = await api.note.createClassic(sub.deck.id, 'go', 'gitmek', [
      { attr_id: attributes[0]!.def.id, value: { type: 'choice', value: b1 } },
    ]);
    expect(note.home_deck_id).toBe(sub.deck.id);
    expect((await api.note.list(sub.deck.id)).items).toHaveLength(1);
    const child = await api.deck.create(`${sub.deck.path}::Düzensiz`);
    expect(child.parent_id).toBe(sub.deck.id);
    expect(events).toEqual(['note_created', 'deck_created']);
  });

  it('rejects unqualified and unknown methods with NOT_FOUND_METHOD', async () => {
    const t = createMockTransport();
    await expect(t.invoke('v1.nope', {})).rejects.toMatchObject({ code: 'NOT_FOUND_METHOD' });
    await expect(t.invoke('decks.tree', {})).rejects.toMatchObject({ code: 'NOT_FOUND_METHOD' });
  });
});
