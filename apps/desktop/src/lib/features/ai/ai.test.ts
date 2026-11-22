import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import AiScreen from './AiScreen.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx = (selectedDeck: string | null, place: ScreenCtx['place'] = 'section'): ScreenCtx => ({
  decks: [],
  place,
  selectedDeck,
  selectedPath: null,
  selectDeck: () => {},
  guarded: async (fn) => {
    try {
      await fn();
      return true;
    } catch {
      return false;
    }
  },
});

describe('<AiScreen> on the dev mock', () => {
  it('previews before sending, then adds the reviewed drafts', async () => {
    const deck = (await api.deck.tree()).roots[0]!.deck.id;

    // A provider first (v1.3: "Add provider" opens the preset gallery, covered in
    // providers.test.ts; here Claude is made directly): without a key nothing can be sent.
    // v1.3: providers are set up on the Settings place of the page, cards made on Create's.
    await api.call('ai.providers.create', { kind: 'anthropic', name: null, base_url: null, default_model: null, is_default: null });
    const settings = render(AiScreen, { ctx: ctx(deck, 'settings') });
    expect(await screen.findByText('No API key yet.')).toBeTruthy();
    expect(screen.queryByLabelText('Source')).toBeNull();
    settings.unmount();

    render(AiScreen, { ctx: ctx(deck) });
    expect(screen.queryByRole('tab', { name: 'Providers' })).toBeNull();
    await fireEvent.input(await screen.findByLabelText('Source'), {
      target: { value: 'Plants turn light into sugar.' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    expect(await screen.findByText('Claude · claude-sonnet-5-5 · 1 item will be sent.')).toBeTruthy();
    expect(screen.getByRole('alert').textContent).toContain('needs an API key');
    expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(true);
    // The full request is there to read.
    expect(screen.getByText('Show exactly what will be sent')).toBeTruthy();
    expect(screen.getAllByText('Plants turn light into sugar.').length).toBeGreaterThan(0);
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    // Store a key (the UI never shows it again), preview again, send.
    const provider = (await api.call('ai.providers.list', {})).providers[0]!;
    await api.call('ai.providers.set_key', { provider_id: provider.id, key: 'sk-test' });
    await fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    const send = await screen.findByRole('button', { name: 'Send' });
    expect((send as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(send);
    expect(await screen.findByText('2 suggestions')).toBeTruthy();
    const before = (await api.call('notes.list', { deck_id: null, include_subdecks: null, page: { cursor: null, limit: 100 } })).items.length;
    await fireEvent.click(screen.getByRole('button', { name: 'Add 2 notes' }));
    expect(await screen.findByText('Added 2 notes, skipped 0.')).toBeTruthy();
    const after = (await api.call('notes.list', { deck_id: null, include_subdecks: null, page: { cursor: null, limit: 100 } })).items.length;
    expect(after - before).toBe(2);
  });

  it('shows usage and the budget state', async () => {
    render(AiScreen, { ctx: ctx(null, 'settings') });
    await fireEvent.click(screen.getByRole('tab', { name: 'Usage' }));
    expect(await screen.findByText('This month')).toBeTruthy();
    expect(screen.getByText('No monthly budget set (Settings → AI).')).toBeTruthy();
  });
});
