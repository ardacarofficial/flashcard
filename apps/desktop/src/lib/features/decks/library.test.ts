import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import LibraryScreen from './LibraryScreen.svelte';
import HomeScreen from '../home/HomeScreen.svelte';
import type { ScreenCtx } from '../types';
import { nodesById } from './library';
import { pageEnter } from './keys';
import { setLauncherHandler, type LaunchRequest } from '../study/launcher.svelte';

i18n.locale = 'en';

const ctx = (over: Partial<ScreenCtx> = {}): ScreenCtx => ({
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
  ...over,
});

describe('v1.1 Library, deck page and Home on the dev mock', () => {
  // v1.3: "Study" opens the study launcher (the shell's LauncherHost registers it).
  const launched = vi.fn<(r: LaunchRequest) => void>();
  let unregister = () => {};
  beforeEach(() => {
    launched.mockReset();
    unregister = setLauncherHandler(launched);
  });
  afterEach(() => unregister());

  it('shows one empty state with one action when there are no decks', async () => {
    render(LibraryScreen, { ctx: ctx() });
    expect(await screen.findByText('No decks yet')).toBeTruthy();
    expect(screen.getAllByRole('button', { name: 'Create your first deck' })).toHaveLength(1);
  });

  it('lists every deck in the tree-table by default, the cards grid on demand', async () => {
    localStorage.removeItem('fc.library.view');
    const decks = (await api.deck.tree()).roots;
    const all = nodesById(decks).size;
    render(LibraryScreen, { ctx: ctx({ decks }) });
    // v1.2: the tree (with sub-decks) is the default view
    expect(await screen.findAllByTestId('deck-row')).toHaveLength(all);
    expect(await screen.findAllByTitle(/% mastered/)).toHaveLength(all);
    expect(screen.getAllByRole('button', { name: /^Study / })).toHaveLength(all);

    // the grid shows the top-level decks as cards, and the choice is remembered
    await fireEvent.click(screen.getByRole('radio', { name: /Cards/ }));
    expect(await screen.findAllByTestId('deck-row')).toHaveLength(decks.length);
    expect(await screen.findAllByText(/% mastered/)).toHaveLength(decks.length);
    expect(localStorage.getItem('fc.library.view')).toBe('grid');
    localStorage.removeItem('fc.library.view');
  });

  it('moves through the tree with the arrow keys and collapses with ←', async () => {
    const decks = (await api.deck.tree()).roots;
    const parent = decks.find((d) => d.children.length > 0)!;
    render(LibraryScreen, { ctx: ctx({ decks }) });
    await screen.findAllByTestId('deck-row');
    const first = screen.getByRole('button', { name: `Open ${decks[0]!.deck.name}` });
    first.focus();
    await fireEvent.keyDown(first, { key: 'ArrowDown' });
    expect(document.activeElement).not.toBe(first);
    const row = screen.getByRole('button', { name: `Open ${parent.deck.name}` });
    row.focus();
    await fireEvent.keyDown(row, { key: 'ArrowLeft' });
    expect(screen.queryByRole('button', { name: `Open ${parent.children[0]!.deck.name}` })).toBeNull();
    await fireEvent.keyDown(screen.getByRole('button', { name: `Open ${parent.deck.name}` }), { key: 'ArrowRight' });
    expect(screen.getByRole('button', { name: `Open ${parent.children[0]!.deck.name}` })).toBeTruthy();
    localStorage.removeItem('fc.library.collapsed');
  });

  it('opens the deck page: modes, term list and inline edit', async () => {
    const decks = (await api.deck.tree()).roots;
    const deck = decks[0]!;
    await api.note.createClassic(deck.deck.id, 'kılıç', 'sword', []);
    render(LibraryScreen, { ctx: ctx({ decks, selectedDeck: deck.deck.id }) });
    expect(await screen.findByRole('heading', { level: 1, name: deck.deck.name })).toBeTruthy();
    expect(await screen.findByRole('button', { name: /Flashcards/ })).toBeTruthy();
    expect((await screen.findAllByText('sword')).length).toBeGreaterThan(0);

    await fireEvent.click(screen.getByRole('button', { name: 'Edit kılıç' }));
    const back = screen.getByLabelText('Definition', { selector: 'textarea' });
    await fireEvent.input(back, { target: { value: 'blade' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect((await screen.findAllByText('blade')).length).toBeGreaterThan(0);
    const notes = await api.call('notes.terms', { deck_id: deck.deck.id, include_subdecks: true, page: { cursor: null, limit: null } });
    expect(notes.items.find((n) => n.front.value === 'kılıç')?.back?.value).toBe('blade');
  });

  it('Home shows today and the start button', async () => {
    const decks = (await api.deck.tree()).roots;
    render(HomeScreen, { ctx: ctx({ decks }) });
    // Nothing is due in the test collection: the button still leads somewhere (practice).
    const start = await screen.findByTestId('home-start');
    expect(start.textContent).toMatch(/Practice|Start studying/);
    expect(await screen.findByText('Today')).toBeTruthy();
    await fireEvent.click(start);
    expect(launched).toHaveBeenCalledWith({ deckId: null });
  });

  it('Home: the words you know lead to Progress; a stale backup shows one line to Settings › Data', async () => {
    const decks = (await api.deck.tree()).roots;
    const navigate = vi.fn();
    render(HomeScreen, { ctx: ctx({ decks, navigate }) });
    // The mock's `words.stats` (312 known, 234 learning) and `io.backup.status` (stale: old).
    const words = await screen.findByTestId('home-words', {}, { timeout: 5000 });
    expect(words.textContent).toMatch(/You know 312 words · learning 234/);
    await fireEvent.click(words);
    expect(navigate).toHaveBeenCalledWith('/stats');
    const backup = await screen.findByTestId('home-backup');
    expect(backup.textContent).toMatch(/Your last backup is old/);
    await fireEvent.click(backup);
    expect(navigate).toHaveBeenCalledWith('/io', 'settings');
  });

  it('Home with a fresh backup: no backup line', async () => {
    const call = api.call.bind(api);
    const spy = vi.spyOn(api, 'call').mockImplementation(((m: string, p: unknown) =>
      m === 'io.backup.status' ? call(m as never, p as never).then((b) => ({ ...(b as object), stale: null })) : call(m as never, p as never)) as never);
    const decks = (await api.deck.tree()).roots;
    render(HomeScreen, { ctx: ctx({ decks }) });
    await screen.findByTestId('home-words', {}, { timeout: 5000 });
    await waitFor(() => expect(spy.mock.calls.some(([m]) => m === 'io.backup.status')).toBe(true));
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByTestId('home-backup')).toBeNull();
    spy.mockRestore();
  });

  it('Home without the study module: no dead Start button, a line that explains', async () => {
    unregister();
    const decks = (await api.deck.tree()).roots;
    render(HomeScreen, { ctx: ctx({ decks }) });
    expect(await screen.findByTestId('home-study-off')).toBeTruthy();
    expect(screen.queryByTestId('home-start')).toBeNull();
  });

  it('deck page: Enter opens the launcher on the deck unless the focus is on a control', async () => {
    const decks = (await api.deck.tree()).roots;
    const deck = decks[0]!;
    const routes: string[] = [];
    let selected: string | null = null;
    render(LibraryScreen, { ctx: ctx({ decks, selectedDeck: deck.deck.id, navigate: (r) => routes.push(r), selectDeck: (id) => (selected = id) }) });
    const study = await screen.findByRole('button', { name: /^Study/ });
    study.focus();
    await fireEvent.keyDown(study, { key: 'Enter' });
    expect(launched).not.toHaveBeenCalled();
    study.blur();
    await fireEvent.keyDown(document.body, { key: 'Enter' });
    // v1.3: never a silent start — the launcher opens on this deck; nothing navigates yet.
    expect(launched).toHaveBeenCalledWith({ deckId: deck.deck.id });
    expect(routes).toEqual([]);
    void selected;
    // A mode button opens the same launcher with that mode chosen.
    await fireEvent.click(screen.getByTestId('deck-mode-test'));
    expect(launched).toHaveBeenLastCalledWith({ deckId: deck.deck.id, kind: 'test' });
  });

  it('pageEnter ignores fields, modifiers and open dialogs', () => {
    const key = (target: EventTarget, init: KeyboardEventInit = {}) => {
      const e = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, ...init });
      Object.defineProperty(e, 'target', { value: target });
      return e;
    };
    const input = document.createElement('input');
    expect(pageEnter(key(document.body))).toBe(true);
    expect(pageEnter(key(input))).toBe(false);
    expect(pageEnter(key(document.body, { ctrlKey: true }))).toBe(false);
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.append(dialog);
    expect(pageEnter(key(document.body))).toBe(false);
    dialog.remove();
  });
});
