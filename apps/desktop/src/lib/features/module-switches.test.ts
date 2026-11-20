/**
 * Switched-off modules on the dev mock (which, like the registry, rejects a disabled module's
 * methods with CONFLICT_MODULE_DISABLED): screens skip those calls and hide their UI instead of
 * showing errors.
 */
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import App from '../../App.svelte';
import BrowseScreen from './browse/BrowseScreen.svelte';
import NoteEditor from './cards/NoteEditor.svelte';
import ImportPanel from './io/ImportPanel.svelte';
import type { ScreenCtx } from './types';

const calls = vi.hoisted(() => [] as string[]);

vi.mock('$lib/api/mock/transport', async (original) => {
  const real = await original<typeof import('$lib/api/mock/transport')>();
  return {
    createMockTransport: () => {
      const t = real.createMockTransport();
      return {
        ...t,
        invoke: (command: string, payload: unknown) => {
          calls.push(command);
          return t.invoke(command, payload);
        },
      };
    },
  };
});

i18n.locale = 'en';

const attributeCalls = () => calls.filter((c) => c.startsWith('v1.attributes.'));

/** A ctx whose `guarded` records failures instead of showing them. */
function ctxWith(over: Partial<ScreenCtx> = {}): ScreenCtx & { failures: unknown[] } {
  const failures: unknown[] = [];
  return {
    decks: [],
    selectedDeck: null,
    selectedPath: null,
    selectDeck: () => {},
    guarded: async (fn) => {
      try {
        await fn();
        return true;
      } catch (e) {
        failures.push(e);
        return false;
      }
    },
    failures,
    ...over,
  };
}

let english = '';

beforeAll(async () => {
  const roots = (await api.deck.tree()).roots;
  english = roots.find((r) => r.deck.name === 'İngilizce')!.deck.id;
  await api.note.createClassic(english, 'kılıç', 'sword', []);
  await api.call('modules.set_enabled', { id: 'attributes', enabled: false });
  await api.call('modules.set_enabled', { id: 'palette', enabled: false });
  calls.length = 0;
});

describe('attributes module off', () => {
  it('the mock rejects attributes.* like the backend', async () => {
    await expect(api.call('attributes.list', {})).rejects.toMatchObject({ code: 'CONFLICT_MODULE_DISABLED' });
    calls.length = 0;
  });

  it('note editor: no classification fields, no attributes calls, no error', async () => {
    render(NoteEditor, { deckId: english, deckName: 'İngilizce' });
    const front = await screen.findByLabelText(/Ön/);
    await waitFor(() => expect(calls).toContain('v1.cards.preview'));
    expect(screen.queryByText('Classifications')).toBeNull();
    expect(screen.queryByLabelText('Seviye')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(attributeCalls()).toEqual([]);

    await fireEvent.input(front, { target: { value: 'kalkan' } });
    await fireEvent.input(screen.getByLabelText(/Arka/), { target: { value: 'shield' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByRole('status')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(attributeCalls()).toEqual([]);
  });

  it('import: no classification columns, no attributes calls, no error', async () => {
    const ctx = ctxWith({ selectedDeck: english, selectedPath: 'İngilizce' });
    render(ImportPanel, { ctx });
    await fireEvent.input(screen.getByRole('textbox', { name: 'Paste text' }), { target: { value: 'a: b' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    const column = (await screen.findByLabelText('Column 1')) as HTMLSelectElement;
    expect(Array.from(column.options, (o) => o.textContent)).not.toContain('Seviye');
    expect(screen.queryByRole('alert')).toBeNull();
    expect(attributeCalls()).toEqual([]);
  });

  it('browser: no bulk classification, no attributes calls, no error', async () => {
    const ctx = ctxWith({ decks: (await api.deck.tree()).roots });
    render(BrowseScreen, { ctx });
    await screen.findByText(/card\(s\)/);
    window.dispatchEvent(new CustomEvent('fc:shortcut', { detail: 'browse.select_all' }));
    // v1.1: the bulk bar's "More actions" menu offers no classification while it is off.
    await fireEvent.click(await screen.findByRole('button', { name: 'More actions' }));
    expect(await screen.findByRole('menuitem', { name: /Find and replace/ })).toBeTruthy();
    expect(screen.queryByRole('menuitem', { name: /Classification/ })).toBeNull();
    expect(ctx.failures).toEqual([]);
    expect(attributeCalls()).toEqual([]);
  });
});

describe('palette module off', () => {
  it('global shortcuts still dispatch; only the palette goes away', async () => {
    render(App);
    const nav = await screen.findByRole('navigation', { name: 'Main navigation' });
    await fireEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    const settings = await screen.findByRole('navigation', { name: 'Settings sections' });
    expect(settings.textContent).toContain('Reminders');
    expect(settings.textContent).not.toContain('Shortcuts');

    // Ctrl+F (ui.search) opens the browser (the Search section).
    await waitFor(async () => {
      await fireEvent.keyDown(window, { key: 'f', code: 'KeyF', ctrlKey: true });
      expect(nav.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Search');
    });
    // Ctrl+K (ui.palette) is still bound but opens nothing.
    await fireEvent.keyDown(window, { key: 'k', code: 'KeyK', ctrlKey: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(calls).not.toContain('v1.palette.list');
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
