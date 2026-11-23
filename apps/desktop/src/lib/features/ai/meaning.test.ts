/**
 * v1.4 "Meaning in this sentence" on the dev mock: shown only with the `ai` module on and a
 * provider set up; "Suggest" previews before anything is sent (or, opted in, asks without a
 * preview and shows the stored suggestion); "Apply" calls `ai.meaning.apply` (read back from
 * the api, undone with `oplog.undo`).
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import NoteEditor from '../cards/NoteEditor.svelte';
import { trackModules } from '../cards/modules.svelte';
import StudyScreen from '../study/StudyScreen.svelte';
import LauncherHost from '../study/LauncherHost.svelte';
import '../study/StudyLauncher.svelte';
import type { ScreenCtx } from '../types';
import MeaningInContext from './MeaningInContext.svelte';
import { aiReady, trackAiReady } from './ready.svelte';

i18n.locale = 'en';

let noteId = '';
let deckId = '';

beforeAll(async () => {
  trackModules();
  deckId = (await api.deck.tree()).roots[0]!.deck.id;
  const n = await api.call('notes.create', {
    notetype_id: '01900000-0000-7000-8000-000000000001',
    deck_id: deckId,
    fields: { front: 'pick up', back: 'kaldırmak' },
    tags: [],
    attributes: [],
    allow_duplicate: true,
  });
  noteId = n.id;
});

afterEach(() => cleanup());

const settle = () => new Promise((r) => setTimeout(r, 30));

describe('meaning in this sentence', () => {
  it('is offered only with the ai module on and a provider set up', async () => {
    trackAiReady();
    await settle();
    expect(aiReady()).toBe(false);
    render(NoteEditor, { deckId, deckName: 'Default', noteId });
    await settle();
    expect(screen.queryByTestId('ai-meaning')).toBeNull();

    const p = await api.call('ai.providers.create', { kind: 'anthropic', name: null, base_url: null, default_model: null, is_default: null });
    await api.call('ai.providers.set_key', { provider_id: p.id, key: 'sk-test' });
    await waitFor(() => expect(aiReady()).toBe(true));
    expect(await screen.findByTestId('ai-meaning')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Suggest' })).toBeTruthy();

    await api.call('modules.set_enabled', { id: 'ai', enabled: false });
    await waitFor(() => expect(aiReady()).toBe(false));
    await waitFor(() => expect(screen.queryByTestId('ai-meaning')).toBeNull());
    await api.call('modules.set_enabled', { id: 'ai', enabled: true });
    await waitFor(() => expect(aiReady()).toBe(true));
  });

  it('previews, then applies the (edited) meaning through the api; undo takes it back', async () => {
    const apply = vi.spyOn(api, 'call');
    render(MeaningInContext, { noteId });
    await fireEvent.click(await screen.findByRole('button', { name: 'Suggest' }));
    // Nothing is sent before the preview is confirmed.
    expect(await screen.findByText('Before sending')).toBeTruthy();
    expect(screen.queryByTestId('ai-meaning-suggestion')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    const box = (await screen.findByLabelText('The meaning that fits')) as HTMLInputElement;
    expect(box.value).toBe('almak (okuldan)');
    expect(screen.getByText('Not in this sentence: kaldırmak')).toBeTruthy();
    expect(screen.getByText('Your card says now: kaldırmak')).toBeTruthy();
    await fireEvent.input(box, { target: { value: 'okuldan almak' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Apply' }));

    await waitFor(() => expect(screen.queryByTestId('ai-meaning-suggestion')).toBeNull());
    const sent = apply.mock.calls.find(([m]) => m === 'ai.meaning.apply')?.[1];
    expect(sent).toMatchObject({ note_id: noteId, meaning: 'okuldan almak', edited: true, confidence: null, accepted: ['okuldan almak'], replace_field: false });
    apply.mockRestore();

    const got = await api.call('ai.meaning.get', { note_id: noteId });
    expect([got.meaning?.status, got.meaning?.meaning, got.meaning?.original]).toEqual(['applied', 'okuldan almak', 'kaldırmak']);
    expect(got.accepted.flatMap((a) => a.answers)).toEqual(['okuldan almak']);
    // The card's own meaning stays (the box was not ticked).
    expect((await api.call('notes.get', { id: noteId })).fields.back).toBe('kaldırmak');
    expect((await screen.findByTestId('ai-meaning-current')).textContent).toContain('okuldan almak');
    expect(screen.getByRole('button', { name: 'Suggest again' })).toBeTruthy();

    await api.call('oplog.undo', {});
    expect((await api.call('ai.meaning.get', { note_id: noteId })).meaning).toBeNull();
    await waitFor(() => expect(screen.queryByTestId('ai-meaning-current')).toBeNull());
  });

  it('opted in to asking without a preview: shows the stored suggestion, dismiss hides it', async () => {
    const real = api.call.bind(api);
    const spy = vi.spyOn(api, 'call').mockImplementation((async (method: string, params: unknown) => {
      if (method === 'settings.get') return { values: [{ key: 'ai.meaning.auto_suggest', value: true, source: null }] };
      return (real as (m: string, p: unknown) => Promise<unknown>)(method, params);
    }) as typeof api.call);
    render(MeaningInContext, { noteId });
    await fireEvent.click(await screen.findByRole('button', { name: 'Suggest' }));
    expect(await screen.findByTestId('ai-meaning-suggestion')).toBeTruthy();
    expect(screen.queryByText('Before sending')).toBeNull();
    expect(spy.mock.calls.some(([m]) => m === 'ai.meaning.suggest')).toBe(true);
    expect((await api.call('ai.meaning.get', { note_id: noteId })).meaning?.status).toBe('suggested');
    await fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByTestId('ai-meaning-suggestion')).toBeNull();
    spy.mockRestore();
  });

  it('the study card menu opens it for the current note', async () => {
    const deck = await api.call('decks.create', { path: 'Anlam', parent_id: null });
    const n = await api.call('notes.create', {
      notetype_id: '01900000-0000-7000-8000-000000000001',
      deck_id: deck.id,
      fields: { front: 'give up', back: 'vazgeçmek' },
      tags: [],
      attributes: [],
    });
    const c: ScreenCtx = { decks: [], selectedDeck: deck.id, selectedPath: null, selectDeck: () => {}, guarded: async (fn) => (await fn(), true) };
    render(LauncherHost, { ctx: c });
    render(StudyScreen, { ctx: c });
    const button = await screen.findByTestId('study-start');
    if (!screen.queryByTestId('study-launcher')) await fireEvent.click(button);
    const start = await screen.findByTestId('launcher-start', {}, { timeout: 5_000 });
    await waitFor(() => expect((start as HTMLButtonElement).disabled).toBe(false));
    await fireEvent.click(start);
    await waitFor(() => expect(screen.getByTestId('question').textContent?.trim()).toBe('give up'));

    const get = vi.spyOn(api, 'call');
    await fireEvent.click(screen.getByRole('button', { name: 'More card actions' }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: 'Meaning in this sentence' }));
    expect(await screen.findByRole('dialog', { name: 'Meaning in this sentence' })).toBeTruthy();
    await waitFor(() => expect(get.mock.calls.some(([m, p]) => m === 'ai.meaning.get' && (p as { note_id: string }).note_id === n.id)).toBe(true));
    get.mockRestore();

    // Without a provider (or with the module off) the entry is not offered.
    await api.call('modules.set_enabled', { id: 'ai', enabled: false });
    await waitFor(() => expect(aiReady()).toBe(false));
    await fireEvent.keyDown(screen.getByRole('dialog', { name: 'Meaning in this sentence' }), { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Meaning in this sentence' })).toBeNull());
    await fireEvent.click(screen.getByRole('button', { name: 'More card actions' }));
    await screen.findAllByRole('menuitem');
    expect(screen.queryByRole('menuitem', { name: 'Meaning in this sentence' })).toBeNull();
    await api.call('modules.set_enabled', { id: 'ai', enabled: true });
  });
});
