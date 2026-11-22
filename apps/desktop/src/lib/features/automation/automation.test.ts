import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import type { DeckTreeNode, RuleTrigger } from '$lib/api/types';
import type { ScreenCtx } from '../types';
import AutomationScreen from './AutomationScreen.svelte';
import { fromAction, fromTrigger, toAction, toTrigger } from './actions';
import { RECIPES, buildCondition, parseCondition, recipeEnv, sentence } from './recipes';

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

const deck = (id: string, path: string): DeckTreeNode =>
  ({ deck: { id, path, name: path.split('::').pop() }, children: [] }) as unknown as DeckTreeNode;

/** Picks `option` in the lib/ui Select labelled `label`. */
async function pick(label: string, option: string) {
  await fireEvent.click(screen.getByRole('combobox', { name: label }));
  await fireEvent.click(screen.getByRole('option', { name: option }));
}

/** Opens the editor's "Advanced: write the rule by hand" disclosure (it remembers its state). */
async function openRaw() {
  const toggle = screen.getByRole('button', { name: /Advanced: write the rule by hand/ });
  if (toggle.getAttribute('aria-expanded') !== 'true') await fireEvent.click(toggle);
}

// Each test starts with the remembered "Advanced" disclosures closed.
beforeEach(() => localStorage.clear());
afterEach(() => vi.restoreAllMocks());

/**
 * Waits until the page has loaded (rules listed, actions known, first recipe counted). Queries
 * afresh: once the rules arrive the sections may swap order, which re-creates the cards.
 */
async function ready() {
  const card = () => screen.getByTestId('recipe-tag_forgotten');
  await waitFor(() => expect(card().textContent).toMatch(/Right now/), { timeout: 8000 });
  return card();
}

// The page counts six recipes with dry runs on load: slow under a loaded test machine.
describe('automation rule editor (dev mock)', { timeout: 20_000 }, () => {
  // v1.3: "New rule" became "Write your own rule", the raw trigger/query/actions editor moved
  // under "Advanced: write the rule by hand", the preview runs by itself (no Preview button)
  // and says how many cards would change; the rules list shows each rule as a sentence and
  // the history says what a run did.
  it('builds a rule by hand, previews it without changing anything, saves and runs it', async () => {
    render(AutomationScreen, { ctx });
    await ready();
    expect(screen.getByText('No rules yet.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Write your own rule' }));
    await fireEvent.input(screen.getByLabelText(/^Name/), { target: { value: 'Leech' } });
    await openRaw();
    await pick('Trigger', 'A card is answered');
    await fireEvent.click(screen.getByLabelText('Again'));
    await fireEvent.input(screen.getByLabelText(/^Cards \(query\)/), { target: { value: 'seviye:A1 lapses>=5' } });
    await fireEvent.input(screen.getByLabelText('Tags (comma separated)'), { target: { value: 'leech, zor' } });

    expect(screen.getByTestId('rule-sentence').textContent).toBe(
      'When you answer a card with “Again”, for cards matching “seviye:A1 lapses>=5”, add the tags “leech”, “zor”.',
    );
    await waitFor(() => expect(screen.getByTestId('preview-count').textContent).toMatch(/^Right now 10 cards would change$/));
    expect(screen.getByText('The preview changes nothing.', { exact: false })).toBeTruthy();
    expect((await api.call('automation.rules.list', {})).rules).toHaveLength(0);

    await fireEvent.click(screen.getByRole('button', { name: 'Save rule' }));
    expect(await screen.findByText('Leech')).toBeTruthy();
    const [rule] = (await api.call('automation.rules.list', {})).rules;
    expect(rule!.trigger).toEqual({ kind: 'card_answered', ratings: ['again'] });
    expect(rule!.condition).toBe('seviye:A1 lapses>=5');
    expect(rule!.actions).toEqual([{ method: 'bulk.add_tags', params: { tags: ['leech', 'zor'] } }]);
    expect(
      screen.getByText('When you answer a card with “Again”, for cards matching “seviye:A1 lapses>=5”, add the tags “leech”, “zor”.'),
    ).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Run now' }));
    const row = await screen.findByTestId('run-row');
    // (The mock clock starts a day back, so the time reads "Yesterday …" or "Today …".)
    expect(row.textContent).toMatch(/(Today|Yesterday) .*· “Leech” · tag added to 10 cards · run by hand/);
  });

  it('offers inbox actions only to inbox rules', async () => {
    render(AutomationScreen, { ctx });
    await ready();
    await fireEvent.click(screen.getByRole('button', { name: 'Write your own rule' }));
    await openRaw();
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
    expect(screen.getByTestId('rule-sentence').textContent).toBe(
      'When a proposal arrives in the inbox, for every proposal, accept it as a note.',
    );
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

describe('automation recipes (dev mock)', { timeout: 20_000 }, () => {
  it('chooses a recipe, shows its live count, adjusts it and saves the expected rule', async () => {
    const call = vi.spyOn(api, 'call');
    render(AutomationScreen, { ctx });
    const card = await ready();
    expect(card.textContent).toContain('Tag cards I keep forgetting');
    // The count is a dry run: automation.preview, nothing saved.
    expect(card.textContent).toContain('Right now 26 cards would change');
    const before = (await api.call('automation.rules.list', {})).rules.length;

    await fireEvent.click(card);
    expect((screen.getByLabelText(/^Name/) as HTMLInputElement).value).toBe('Tag cards I keep forgetting');
    const sentenceEl = screen.getByTestId('rule-sentence');
    expect(sentenceEl.textContent).toBe(
      'When you answer a card with “Again”, for cards you have forgotten 4+ times, add the tag “difficult”.',
    );
    await waitFor(() => expect(screen.getByTestId('preview-count').textContent).toBe('Right now 26 cards would change'));
    expect(call).toHaveBeenCalledWith('automation.preview', {
      rule_id: null,
      draft: {
        trigger: { kind: 'card_answered', ratings: ['again'] },
        condition: 'lapses>=4',
        actions: [{ method: 'bulk.add_tags', params: { tags: ['difficult'] } }],
      },
    });

    // Edit a part in place: the threshold.
    await fireEvent.click(within(sentenceEl).getByRole('button', { name: 'cards you have forgotten 4+ times' }));
    await fireEvent.input(screen.getByLabelText('Forgotten at least (times)'), { target: { value: '6' } });
    expect(sentenceEl.textContent).toContain('for cards you have forgotten 6+ times');
    await waitFor(() => expect(screen.getByTestId('preview-count').textContent).toBe('Right now 16 cards would change'));
    // …and the tag.
    await fireEvent.click(within(sentenceEl).getByRole('button', { name: 'add the tag “difficult”' }));
    await fireEvent.input(screen.getByLabelText('Tags (comma separated)'), { target: { value: 'leech' } });
    expect(sentenceEl.textContent).toContain('add the tag “leech”');

    await fireEvent.click(screen.getByRole('button', { name: 'Save rule' }));
    await waitFor(() =>
      expect(call).toHaveBeenCalledWith('automation.rules.create', {
        name: 'Tag cards I keep forgetting',
        enabled: true,
        trigger: { kind: 'card_answered', ratings: ['again'] },
        condition: 'lapses>=6',
        actions: [{ method: 'bulk.add_tags', params: { tags: ['leech'] } }],
      }),
    );
    expect((await api.call('automation.rules.list', {})).rules).toHaveLength(before + 1);
  });

  it('asks for a deck before an archive rule can be saved', async () => {
    render(AutomationScreen, { ctx: { ...ctx, decks: [deck('d-arch', 'Old words'), deck('d-en', 'English')] } });
    await ready();
    await fireEvent.click(screen.getByTestId('recipe-archive_learned'));
    expect(screen.getByTestId('rule-sentence').textContent).toBe(
      'When you answer a card, for cards you will remember for 90+ days, move them to a deck (choose one).',
    );
    expect(await screen.findByText('Choose the deck the cards should go to.')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Save rule' }) as HTMLButtonElement).disabled).toBe(true);

    // The part that still needs a choice is already open.
    expect(screen.getByRole('button', { name: 'move them to a deck (choose one)' }).getAttribute('aria-expanded')).toBe('true');
    await pick('Deck', 'Old words');
    expect(screen.getByTestId('rule-sentence').textContent).toContain('move them to “Old words”');
    await waitFor(() => expect(screen.getByTestId('preview-count').textContent).toMatch(/^Right now \d+ cards would change$/));
    expect((screen.getByRole('button', { name: 'Save rule' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('confirms before deleting a rule', async () => {
    await api.call('automation.rules.create', {
      name: 'Doomed',
      enabled: false,
      trigger: { kind: 'note_added' },
      condition: '',
      actions: [{ method: 'bulk.add_tags', params: { tags: ['x'] } }],
    });
    render(AutomationScreen, { ctx });
    const row = (await screen.findByText('Doomed')).closest('li')!;
    await fireEvent.click(within(row as HTMLElement).getByRole('button', { name: 'Delete' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(dialog.textContent).toContain('Delete “Doomed”?');
    expect((await api.call('automation.rules.list', {})).rules.some((r) => r.name === 'Doomed')).toBe(true);
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));
    await waitFor(async () => expect((await api.call('automation.rules.list', {})).rules.some((r) => r.name === 'Doomed')).toBe(false));
  });

  it('reads the recipe conditions as words and writes them back unchanged', () => {
    const env = recipeEnv([], []);
    for (const r of RECIPES) {
      const f = r.form(env);
      expect(buildCondition(parseCondition(f.condition))).toBe(f.condition);
      expect(parseCondition(f.condition).kind).not.toBe('query');
    }
    expect(parseCondition('lapses>=5 seviye:A1').kind).toBe('query');
    expect(parseCondition('').kind).toBe('all');
    const untag = RECIPES.find((r) => r.id === 'untag_learned')!.form(env);
    expect(sentence(untag, { decks: [], attrs: [], subtitleType: env.subtitleType })).toBe(
      'Once a day, for cards tagged “difficult” that you will remember for 30+ days, remove the tag “difficult”.',
    );
    const sub = RECIPES.find((r) => r.id === 'tag_subtitle')!.form(env);
    expect(sub.condition).toBe('notetype:"Subtitle card"');
    expect(sentence(sub, { decks: [], attrs: [], subtitleType: env.subtitleType })).toBe(
      'When a new note is added, for cards that came from Subtitle, add the tag “subtitle”.',
    );
  });

  it('shows the loop limits with what they do, a default marker and reset', async () => {
    const defs = [
      {
        key: 'automation.max_depth',
        kind: { type: 'integer', min: 1, max: 10 },
        default: 3,
        label_key: 'settings.automation.max_depth.label',
        description_key: 'settings.automation.max_depth.description',
        advanced: true,
        scopes: ['app'],
      },
    ];
    const real = api.call.bind(api);
    let stored: number | null = 5;
    vi.spyOn(api, 'call').mockImplementation(((method: string, req: unknown) => {
      if (method === 'settings.schema') return Promise.resolve({ settings: defs.map((def) => ({ module: 'automation', def })) });
      if (method === 'settings.get') return Promise.resolve({ values: [{ key: 'automation.max_depth', value: stored, source: null }] });
      if (method === 'settings.reset') {
        stored = null;
        return Promise.resolve({ key: 'automation.max_depth', value: 3, source: null });
      }
      return real(method as never, req as never);
    }) as typeof api.call);
    const { resetDefaultsCache } = await import('../core/kit/defaults.svelte');
    resetDefaultsCache();
    render(AutomationScreen, { ctx });
    const toggle = await screen.findByRole('button', { name: /Advanced: rule chains/ });
    if (toggle.getAttribute('aria-expanded') !== 'true') await fireEvent.click(toggle);
    expect(screen.getByText(/can never loop forever/)).toBeTruthy();
    expect(screen.getByText(/rule A may start B and B may start C/)).toBeTruthy();
    const input = screen.getByLabelText('Longest chain of rules') as HTMLInputElement;
    expect(input.value).toBe('5');
    await fireEvent.click(await screen.findByRole('button', { name: 'Reset to default' }));
    await waitFor(() => expect(input.value).toBe('3'));
    expect(screen.queryByRole('button', { name: 'Reset to default' })).toBeNull();
  });
});
