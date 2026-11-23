import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
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

    await fireEvent.click(screen.getByRole('combobox', { name: 'Choose a deck' }));
    await fireEvent.click(await screen.findByRole('option', { name: decks[0]!.deck.path }));
    await fireEvent.click(screen.getByRole('button', { name: 'Use for deck' }));
    await screen.findByRole('button', { name: `Remove from “${decks[0]!.deck.path}”` });
    const eff = await api.call('presets.for_deck', { deck_id: decks[0]!.deck.id });
    expect(eff.preset_id).toBe(p.id);
  }, 15000);
});

describe('<PresetsScreen> v1.3: explained, defaults marked, rarely used rows tucked away', () => {
  it('keeps rarely used rows in a closed “Advanced” disclosure until it is opened', async () => {
    localStorage.clear();
    if ((await api.call('presets.list', {})).items.length === 0) await api.call('presets.create', { name: 'Daily', config: null });
    render(PresetsScreen, { ctx });
    expect(await screen.findByLabelText('New cards per day')).toBeTruthy();
    // Everyday rows are visible; the rest is behind the closed disclosure.
    expect(screen.getByLabelText('Desired retention')).toBeTruthy();
    expect(screen.getByLabelText('Learning steps (minutes)')).toBeTruthy();
    const toggle = screen.getByRole('button', { name: /More settings/ });
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    for (const label of ['Maximum interval (days)', 'FSRS parameters', 'New card order', 'Review order']) {
      expect(screen.queryByLabelText(label)).toBeNull();
    }
    expect(screen.queryByRole('switch', { name: 'Leave related cards for tomorrow' })).toBeNull();
    await fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByLabelText('Maximum interval (days)')).toBeTruthy();
    expect(screen.getByRole('switch', { name: 'Leave related cards for tomorrow' })).toBeTruthy();
    localStorage.clear();
  });

  it('explains what a preset is, marks defaults and puts a default back', async () => {
    localStorage.clear();
    const decks = (await api.deck.tree()).roots;
    render(PresetsScreen, { ctx: { ...ctx, decks } });
    expect(await screen.findByText('What is a preset?')).toBeTruthy();
    const perDay = (await screen.findByLabelText('New cards per day')) as HTMLInputElement;
    await fireEvent.input(perDay, { target: { value: '35' } });
    expect(await screen.findByText(/35 → at most 35 new words are asked each day/)).toBeTruthy();
    expect(screen.getByText(/You have unsaved changes/)).toBeTruthy();
    const row = perDay.closest('.fc-form-row') as HTMLElement;
    expect(within(row).queryByText('default')).toBeNull();
    await fireEvent.click(within(row).getByRole('button', { name: 'Reset to default' }));
    expect(perDay.value).toBe('20');
    expect(within(row).getByText('default')).toBeTruthy();
    expect(within(row).queryByRole('button', { name: 'Reset to default' })).toBeNull();
  });
});
