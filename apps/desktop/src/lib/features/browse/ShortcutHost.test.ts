/**
 * v1.3 journey J7: keys typed into a text field belong to the field. With the Search screen
 * and the shell's shortcut host on the dev mock, Ctrl+A / Ctrl+Z / Delete inside the search
 * field must not select rows, run `oplog.undo` or ask to delete — the same keys outside a
 * field still do their job. A held key is one action.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import BrowseScreen from './BrowseScreen.svelte';
import ShortcutHost from './ShortcutHost.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const guarded: ScreenCtx['guarded'] = async (fn) => {
  try {
    await fn();
    return true;
  } catch {
    return false;
  }
};

let seeded = false;

async function setup() {
  const decks = (await api.deck.tree()).roots;
  if (!seeded) {
    await api.note.createClassic(decks[0]!.deck.id, 'kalem', 'pencil', []);
    await api.note.createClassic(decks[0]!.deck.id, 'pusula', 'compass', []);
    seeded = true;
  }
  const call = vi.spyOn(api, 'call');
  const undo = vi.spyOn(api.oplog, 'undo').mockResolvedValue(undefined as never);
  const redo = vi.spyOn(api.oplog, 'redo').mockResolvedValue(undefined as never);
  render(ShortcutHost, { screens: [], activeScreen: 'browse', guarded });
  render(BrowseScreen, { ctx: { decks, selectedDeck: null, selectedPath: null, selectDeck: () => {}, guarded } });
  await screen.findAllByTestId('browse-row');
  const field = screen.getByLabelText('Search', { selector: 'input' });
  const selectAllCalls = () => call.mock.calls.filter(([m]) => m === 'search.ids').length;
  // The bindings load asynchronously: Ctrl+A on the page (not in a field) selects all rows.
  await waitFor(async () => {
    await fireEvent.keyDown(document.body, { key: 'a', code: 'KeyA', ctrlKey: true });
    expect(selectAllCalls()).toBeGreaterThan(0);
  });
  await fireEvent.keyDown(document.body, { key: 'Escape' }); // clear the selection again
  call.mockClear();
  return { field, undo, redo, call, selectAllCalls };
}

const checked = () => screen.getAllByTestId('browse-select').filter((s) => s.querySelector('input')?.checked).length;

describe('<ShortcutHost> keys inside a text field (J7)', () => {
  it('Ctrl+A, Ctrl+Z, Ctrl+Shift+Z, Delete and the other Ctrl keys in the search field edit the text, not the app', async () => {
    const { field, undo, redo, call, selectAllCalls } = await setup();
    // Rows selected on the page first (so Delete would have something to delete).
    await fireEvent.keyDown(document.body, { key: 'a', code: 'KeyA', ctrlKey: true });
    await waitFor(() => expect(checked()).toBe(2));
    call.mockClear();
    const forwarded: string[] = [];
    const onShortcut = (e: Event) => forwarded.push((e as CustomEvent<string>).detail);
    window.addEventListener('fc:shortcut', onShortcut);

    field.focus();
    const a = await fireEvent.keyDown(field, { key: 'a', code: 'KeyA', ctrlKey: true });
    const z = await fireEvent.keyDown(field, { key: 'z', code: 'KeyZ', ctrlKey: true });
    await fireEvent.keyDown(field, { key: 'Z', code: 'KeyZ', ctrlKey: true, shiftKey: true });
    await fireEvent.keyDown(field, { key: 'Delete' });
    for (const k of ['j', 'i', 'n', 'f']) await fireEvent.keyDown(field, { key: k, code: `Key${k.toUpperCase()}`, ctrlKey: true });
    await new Promise((r) => setTimeout(r, 50));
    window.removeEventListener('fc:shortcut', onShortcut);
    // Native editing wins: the events are not cancelled, nothing reaches the app.
    expect(a).toBe(true);
    expect(z).toBe(true);
    expect(selectAllCalls()).toBe(0);
    expect(call.mock.calls.map(([m]) => m)).not.toContain('bulk.set_suspended');
    expect(forwarded).toEqual([]);
    expect(undo).not.toHaveBeenCalled();
    expect(redo).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(checked()).toBe(2);

    // The same Ctrl+Z on the page is the app's undo.
    await fireEvent.keyDown(document.body, { key: 'z', code: 'KeyZ', ctrlKey: true });
    expect(undo).toHaveBeenCalledTimes(1);
  });

  it('a held Ctrl+Z undoes once', async () => {
    const { undo } = await setup();
    await fireEvent.keyDown(document.body, { key: 'z', code: 'KeyZ', ctrlKey: true });
    for (let i = 0; i < 3; i++) await fireEvent.keyDown(document.body, { key: 'z', code: 'KeyZ', ctrlKey: true, repeat: true });
    expect(undo).toHaveBeenCalledTimes(1);
  });

  it('a contenteditable element keeps Ctrl+Z too', async () => {
    const { undo } = await setup();
    const editor = document.body.appendChild(document.createElement('div'));
    editor.setAttribute('contenteditable', 'true');
    await fireEvent.keyDown(editor, { key: 'z', code: 'KeyZ', ctrlKey: true });
    expect(undo).not.toHaveBeenCalled();
    editor.remove();
  });

  it('the palette key still opens from a field', async () => {
    const { field } = await setup();
    field.focus();
    await fireEvent.keyDown(field, { key: 'k', code: 'KeyK', ctrlKey: true });
    expect(await screen.findByRole('dialog', { name: 'Command palette' })).toBeTruthy();
  });
});

describe('<BrowseScreen> held keys on the table', () => {
  it('a held Space toggles one row once', async () => {
    await setup();
    const grid = screen.getByRole('grid');
    grid.focus();
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: ' ' });
    for (let i = 0; i < 3; i++) await fireEvent.keyDown(grid, { key: ' ', repeat: true });
    await waitFor(() => expect(checked()).toBe(1));
  });
});
