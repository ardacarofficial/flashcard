import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import type { RuleTrigger } from '$lib/api/types';
import type { ScreenCtx } from '../types';
import AutomationScreen from './AutomationScreen.svelte';
import { fromAction, fromTrigger, toAction, toTrigger } from './actions';

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

/** Picks `option` in the lib/ui Select labelled `label`. */
async function pick(label: string, option: string) {
  await fireEvent.click(screen.getByRole('combobox', { name: label }));
  await fireEvent.click(screen.getByRole('option', { name: option }));
}

describe('automation rule editor (dev mock)', () => {
  it('builds a rule, previews it without changing anything, saves and runs it', async () => {
    render(AutomationScreen, { ctx });
    expect(await screen.findByText('No rules yet.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'New rule' }));
    await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Leech' } });
    await pick('Trigger', 'A card is answered');
    await fireEvent.click(screen.getByLabelText('Again'));
    await fireEvent.input(screen.getByLabelText('Cards (query)'), { target: { value: 'seviye:A1 lapses>=5' } });
    await fireEvent.input(screen.getByLabelText('Tags (comma separated)'), { target: { value: 'leech, zor' } });

    await fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    expect(await screen.findByText('2 cards match, 1 would change. Nothing has been changed.')).toBeTruthy();
    expect(screen.getByText('would change')).toBeTruthy();
    expect((await api.call('automation.rules.list', {})).rules).toHaveLength(0);

    await fireEvent.click(screen.getByRole('button', { name: 'Save rule' }));
    expect(await screen.findByText('Leech')).toBeTruthy();
    const [rule] = (await api.call('automation.rules.list', {})).rules;
    expect(rule!.trigger).toEqual({ kind: 'card_answered', ratings: ['again'] });
    expect(rule!.condition).toBe('seviye:A1 lapses>=5');
    expect(rule!.actions).toEqual([{ method: 'bulk.add_tags', params: { tags: ['leech', 'zor'] } }]);
    expect(screen.getByText('A card is answered · Again')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Run now' }));
    expect(await screen.findByText(/applied · By hand · 3 cards/)).toBeTruthy();
  });

  it('offers inbox actions only to inbox rules', async () => {
    render(AutomationScreen, { ctx });
    await fireEvent.click(await screen.findByRole('button', { name: 'New rule' }));
    const options = async () => {
      const box = screen.getByRole('combobox', { name: 'Action' });
      await fireEvent.click(box);
      const labels = screen.getAllByRole('option').map((o) => o.textContent?.trim());
      await fireEvent.keyDown(box, { key: 'Escape' });
      return labels;
    };
    expect(await options()).not.toContain('Accept as a note');
    await pick('Trigger', 'A proposal arrives in the inbox');
    expect(await options()).toEqual(['Accept as a note']);
    expect(screen.getByRole('combobox', { name: 'Action' }).textContent).toContain('Accept as a note');
    expect(screen.getByText(/the card the proposal would become/)).toBeTruthy();
    // Accepting without review is called out in the editor.
    await waitFor(() => expect(screen.getByText(/This rule accepts proposals without your review/)).toBeTruthy());
  });

  it('round-trips triggers and action params', () => {
    const triggers: RuleTrigger[] = [
      { kind: 'schedule', every_mins: 90 },
      { kind: 'card_flagged', flags: [1, 7] },
      { kind: 'imported' },
      { kind: 'inbox_added' },
    ];
    for (const t of triggers) {
      expect(toTrigger(fromTrigger(t))).toEqual(t);
    }
    for (const a of [
      { method: 'bulk.set_flag', params: { flag: 3, on: false } },
      { method: 'bulk.move_cards', params: { deck_id: 'd1' } },
      { method: 'bulk.set_attribute', params: { attr_id: 'a1', value: { type: 'choice', value: 'o1' } } },
      { method: 'bulk.set_suspended', params: { suspended: true } },
      { method: 'bulk.reset_progress', params: {} },
      { method: 'inbox.accept', params: { deck_id: 'd1' } },
      { method: 'inbox.accept', params: {} },
    ]) {
      expect(toAction(fromAction(a))).toEqual(a);
    }
  });
});
