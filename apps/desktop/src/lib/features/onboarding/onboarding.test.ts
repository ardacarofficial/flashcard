import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import OnboardingHost from './OnboardingHost.svelte';
import StartersScreen from './StartersScreen.svelte';
import type { ScreenCtx } from '../types';
import { guide, openGuide, resetGuide } from './guide.svelte';

// Whether the collection counts as new (a test of the reopened guide turns it off).
const flags = vi.hoisted(() => ({ fresh: true }));

// The dev mock already holds decks (so it never shows the guide); pretend a new collection.
vi.mock('$lib/api/mock/transport', async (original) => {
  const real = await original<typeof import('$lib/api/mock/transport')>();
  return {
    createMockTransport: () => {
      const t = real.createMockTransport();
      return {
        ...t,
        invoke: async (command: string, payload: unknown) => {
          if (command === 'v1.onboarding.status' && flags.fresh) return { show: true, completed: false };
          if (command === 'v1.onboarding.apply' || command === 'v1.onboarding.skip') flags.fresh = false;
          return t.invoke(command, payload);
        },
      };
    },
  };
});

i18n.locale = 'en';

beforeEach(() => {
  flags.fresh = true;
  resetGuide();
});

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

describe('first-run guide (dev mock)', () => {
  it('asks what you study, shows the matching classification and sets up deck + starters', async () => {
    const ondone = vi.fn();
    render(OnboardingHost, { ondone });
    expect(await screen.findByRole('heading', { name: 'Welcome to Flashcard' }, { timeout: 5000 })).toBeTruthy();
    // v1.3: one screen (no "Next"); English is picked first (the main user learns English).
    expect(screen.getByRole('radio', { name: 'English' }).getAttribute('aria-checked')).toBe('true');
    // English → CEFR; YKS → the exam sections.
    expect(screen.getAllByText('Classification: CEFR levels (A1–C2)').length).toBe(2); // English, other language
    expect(screen.getByText('Classification: YKS sections (TYT / AYT / YDT)')).toBeTruthy();
    await fireEvent.click(screen.getByLabelText(/^English/));

    const name = screen.getByLabelText('Deck name') as HTMLInputElement;
    expect(name.value).toBe('English');
    await fireEvent.input(name, { target: { value: 'My English' } });
    const everyday = screen.getByLabelText(/Everyday English/) as HTMLInputElement;
    const appGuide = screen.getByLabelText(/How Flashcard works/) as HTMLInputElement;
    expect(everyday.checked && appGuide.checked).toBe(true);
    await fireEvent.click(appGuide);
    await fireEvent.click(screen.getByRole('button', { name: 'Start with a ready deck' }));
    expect(await screen.findByText('Ready: 24 cards added. Undo removes the whole setup.')).toBeTruthy();
    expect(ondone).toHaveBeenCalled();

    const roots = (await api.deck.tree()).roots;
    const mine = roots.find((r) => r.deck.name === 'My English')!;
    expect(mine.children.map((c) => c.deck.name)).toEqual(['Everyday English']);
    expect((await api.call('onboarding.status', {})).completed).toBe(true);
  });

  it('after setup shows a non-modal checklist of four steps, each opening its page', async () => {
    const navigate = vi.fn();
    const selectDeck = vi.fn();
    const home = vi.fn();
    render(OnboardingHost, { ctx: { ...ctx, navigate, selectDeck }, home });
    await fireEvent.click(await screen.findByRole('button', { name: 'Start with a ready deck' }, { timeout: 5000 }));
    let panel = await screen.findByRole('complementary', { name: /Getting started/ });
    expect(document.querySelector('[aria-modal="true"]')).toBeNull();
    // Step 1 is done (the setup); four steps, one of them optional.
    expect(panel.textContent).toContain('1/4');
    for (const step of ['Add your first words', 'Study', 'Where today’s cards and your progress live', 'Connect the browser extension']) {
      expect(screen.getByRole('heading', { name: new RegExp(`^${step}`) })).toBeTruthy();
    }
    expect(screen.getByRole('heading', { name: /Connect the browser extension.*optional/ })).toBeTruthy();
    expect(guide.done).toEqual(['add']);
    // Only the next step explains itself; the others stay one line.
    expect(panel.textContent).toContain('Cards come one by one');
    expect(panel.textContent).not.toContain('Home shows the cards due today.');

    // Each step's button does the thing, ticks the step and folds the checklist away.
    const reopen = async () => {
      expect(screen.queryByRole('complementary')).toBeNull();
      await fireEvent.click(screen.getByRole('button', { name: /Getting started/ }));
      return screen.getByRole('complementary', { name: /Getting started/ });
    };
    await fireEvent.click(screen.getByRole('button', { name: 'Study' }));
    expect(navigate).toHaveBeenLastCalledWith('/study');
    expect(selectDeck).toHaveBeenCalled();
    await reopen();
    await fireEvent.click(screen.getByRole('button', { name: 'Home' }));
    expect(home).toHaveBeenCalled();
    await reopen();
    await fireEvent.click(screen.getByRole('button', { name: 'Progress' }));
    expect(navigate).toHaveBeenLastCalledWith('/stats');
    await reopen();
    await fireEvent.click(screen.getByRole('button', { name: 'Connect' }));
    expect(navigate).toHaveBeenLastCalledWith('/subtitle');
    expect(guide.done).toEqual(['add', 'study', 'where', 'extension']);
    panel = await reopen();
    expect(panel.textContent).toContain('4/4');
    expect(panel.textContent).toContain('All done!');
    // Persisted (UI state, this device only).
    expect(JSON.parse(localStorage.getItem('fc.guide')!).done).toHaveLength(4);

    // Esc hides it to a small button; that button brings it back.
    await fireEvent.keyDown(panel, { key: 'Escape' });
    expect(screen.queryByRole('complementary')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: /Getting started/ }));
    expect(screen.getByRole('complementary', { name: /Getting started/ })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Finish the guide' }));
    expect(screen.queryByRole('complementary')).toBeNull();
    expect(guide.view).toBe('closed');
  });

  it('“I’ll add my own words” sets up an empty deck and opens the add page on it', async () => {
    const navigate = vi.fn();
    const selected: (string | null)[] = [];
    let decks: ScreenCtx['decks'] = [];
    const { rerender } = render(OnboardingHost, {
      ctx: { ...ctx, navigate, selectDeck: (id) => selected.push(id) },
      ondone: () => {
        void api.deck.tree().then((t) => {
          decks = t.roots;
          void rerender({ ctx: { ...ctx, decks, navigate, selectDeck: (id) => selected.push(id) } });
        });
      },
    });
    const own = await screen.findByRole('button', { name: 'I’ll add my own words' }, { timeout: 5000 });
    const before = (await api.deck.tree()).roots.map((r) => r.deck.id);
    await fireEvent.click(own);
    await waitFor(() => expect(navigate).toHaveBeenCalledWith('/notes/new'));
    const added = (await api.deck.tree()).roots.filter((r) => !before.includes(r.deck.id));
    expect(added.map((r) => r.deck.name)).toEqual(['English']); // the deck only, no starter decks
    const deck = added[0]!;
    expect(deck.children).toEqual([]);
    expect(selected.at(-1)).toBe(deck.deck.id);
    // v1.3: an empty deck is not "first words added": step 1 stays open and the checklist
    // starts folded (it must not cover the add page the user is about to type on) …
    expect(guide.done).not.toContain('add');
    expect(guide.view).toBe('pill');
    // … and ticks itself once the collection holds a card.
    const shown = { ...ctx, navigate, decks: [{ ...deck, counts: { ...deck.counts, total: 1 } }] };
    await rerender({ ctx: shown });
    await waitFor(() => expect(guide.done).toContain('add'));
  });

  it('skipping before setup calls onboarding.skip and closes the guide', async () => {
    const skip = vi.spyOn(api, 'call');
    const ondone = vi.fn();
    render(OnboardingHost, { ondone });
    await fireEvent.click(await screen.findByRole('button', { name: 'Skip the guide' }, { timeout: 5000 }));
    await waitFor(() => expect(ondone).toHaveBeenCalled());
    expect(skip.mock.calls.some(([m]) => m === 'onboarding.skip')).toBe(true);
    expect(screen.queryByRole('complementary')).toBeNull();
    expect(guide.view).toBe('closed');
    skip.mockRestore();
  });

  it('openGuide() reopens the checklist; with decks, step 1 opens the add page', async () => {
    flags.fresh = false;
    const navigate = vi.fn();
    const decks = (await api.deck.tree()).roots;
    render(OnboardingHost, { ctx: { ...ctx, decks, navigate } });
    expect(screen.queryByRole('complementary')).toBeNull();
    openGuide();
    flushSync();
    const panel = await screen.findByRole('complementary', { name: /Getting started/ });
    expect(panel.textContent).toContain('0/4');
    await waitFor(() => expect(document.activeElement).toBe(panel));
    await fireEvent.click(screen.getByRole('button', { name: 'Add cards' }));
    expect(navigate).toHaveBeenCalledWith('/notes/new');
    expect(guide.done).toEqual(['add']);
  });

  it('the starters page reopens the guide', async () => {
    render(StartersScreen, { ctx });
    await fireEvent.click(screen.getByRole('button', { name: 'Open the getting-started guide' }));
    expect(guide.view).toBe('panel');
  });

  it('starter decks can be added later from their screen', async () => {
    render(StartersScreen, { ctx });
    const add = await screen.findAllByRole('button', { name: 'Add' });
    expect(add.length).toBe(4);
    await fireEvent.click(add[2]!);
    expect(await screen.findByText('Added “World capitals” (16 cards).')).toBeTruthy();
  });
});
