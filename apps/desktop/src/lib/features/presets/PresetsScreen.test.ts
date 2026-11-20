import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import PresetsScreen from './PresetsScreen.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx: ScreenCtx = {
  decks: [],
  selectedDeck: null,
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
};

describe('<PresetsScreen> on the dev mock', () => {
  it('creates a preset, edits it and assigns it to a deck', async () => {
    const decks = (await api.deck.tree()).roots;
    render(PresetsScreen, { ctx: { ...ctx, decks } });
    expect(await screen.findByText('No presets yet: every deck uses the defaults.')).toBeTruthy();

    await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Exam week' } });
    await fireEvent.click(screen.getByRole('button', { name: 'New preset' }));
    const perDay = (await screen.findByLabelText('New cards per day')) as HTMLInputElement;
    expect(perDay.value).toBe('20');
    await fireEvent.input(perDay, { target: { value: '50' } });
    await fireEvent.input(screen.getByLabelText('Learning steps (minutes)'), { target: { value: '1 10 60' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await screen.findByText('Saved.');
    const p = (await api.call('presets.list', {})).items[0]!;
    expect(p.config.new_per_day).toBe(50);
    expect(p.config.learning_steps_mins).toEqual([1, 10, 60]);

    const pick = screen.getByLabelText('Choose a deck') as HTMLSelectElement;
    await fireEvent.change(pick, { target: { value: decks[0]!.deck.id } });
    await fireEvent.click(screen.getByRole('button', { name: 'Use for deck' }));
    await screen.findByRole('button', { name: `Remove from “${decks[0]!.deck.path}”` });
    const eff = await api.call('presets.for_deck', { deck_id: decks[0]!.deck.id });
    expect(eff.preset_id).toBe(p.id);
  });
});
