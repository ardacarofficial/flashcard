import { describe, expect, it } from 'vitest';
import { configure, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api, CLASSIC_NOTETYPE_ID } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import App from './App.svelte';

i18n.locale = 'en';
// The whole mock app boots per test: on a loaded machine that takes longer than the 1 s default.
configure({ asyncUtilTimeout: 10_000 });

describe('<App> on the dev mock transport', { timeout: 30_000 }, () => {
  it('draws the 5 sections + Inbox + Settings + profile from modules.list; section pages are tabs', async () => {
    render(App);
    const nav = await screen.findByRole('navigation', { name: 'Main navigation' });
    await screen.findByRole('button', { name: 'Settings' });
    const labels = () => Array.from(nav.querySelectorAll('li > button'), (b) => b.getAttribute('aria-label') ?? b.textContent?.trim());
    await waitFor(() =>
      expect(labels()).toEqual(['Home', 'Library', 'Create', 'Search', 'Statistics', expect.stringMatching(/^Inbox/), 'Settings']),
    );
    expect(screen.getByRole('button', { name: /^Profile: / })).toBeTruthy();
    // Home opens first (bundled) and is built from $lib/ui.
    expect(await screen.findByRole('heading', { level: 1, name: 'Home' })).toBeTruthy();

    // Create: Add + the pages placed there, as tabs.
    await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    const tabs = await screen.findByRole('tablist', { name: 'Pages in Create' });
    expect(Array.from(tabs.querySelectorAll('[role=tab]'), (t) => t.textContent?.trim())).toEqual(['Add', 'AI', 'Starter decks']);
    expect(nav.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Create');

    // v1.1 Create: a new deck or an existing one (design-system Select); classifications
    // follow the target deck — "Seviye" (A1–C2) is inherited from the parent deck.
    await fireEvent.click(await screen.findByRole('radio', { name: 'Existing deck' }));
    await fireEvent.click(await screen.findByRole('combobox', { name: 'Deck' }));
    await fireEvent.click(await screen.findByRole('option', { name: 'İngilizce::Fiiller' }));
    // v1.2: classifications sit under "More" in the bulk editor's one setup row.
    await fireEvent.click(await screen.findByRole('button', { name: 'More' }));
    await fireEvent.click(await screen.findByRole('combobox', { name: 'Seviye' }));
    expect(screen.getAllByRole('option').map((o) => o.textContent?.trim())).toEqual([
      'Not set', 'A1', 'A2', 'B1', 'B2', 'C1', 'C2',
    ]);
  });

  it('Settings hosts the admin screens in groups; every v1.0 screen stays reachable', async () => {
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    const sub = await screen.findByRole('navigation', { name: 'Settings sections' });
    const groups = Array.from(sub.querySelectorAll('h2'), (h) => h.textContent?.trim());
    // v1.3: grouped by task, technical pages apart under "Advanced".
    expect(groups).toEqual(['General', 'Studying and cards', 'Connections', 'Data', 'Advanced']);
    const pages = Array.from(sub.querySelectorAll('button'), (b) => b.textContent?.trim());
    // Subtitle: off by default in the app, on in the dev mock (its settings page gets screenshots).
    expect(pages).toEqual(['General', 'Reminders', 'Shortcuts', 'Profiles & lock', 'Deck options', 'Note types', 'Modes and reading', 'AI', 'Subtitle', 'Export & backup', 'Starter decks', 'Automation', 'Assistants (MCP)', 'Plugins', 'Maintenance', 'Logs & updates']);
    await fireEvent.click(screen.getByRole('button', { name: 'Reminders' }));
    expect(sub.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Reminders');

    // The other v1.0 screens live under the sections.
    const tabsOf = async (section: string) => {
      await fireEvent.click(screen.getByRole('button', { name: section }));
      // The tabs are drawn by the (lazy) screen's PageHeader, under its title.
      await waitFor(() => expect(document.querySelector('main h1')).toBeTruthy());
      const list = screen.queryByRole('tablist');
      return list ? Array.from(list.querySelectorAll('[role=tab]'), (t) => t.textContent?.trim()) : [];
    };
    // Library: Decks | Notes only; the study modes open from the deck page's tiles and Home.
    expect(await tabsOf('Library')).toEqual(['Decks', 'Notes']);
    expect(await tabsOf('Search')).toEqual([]);
    expect(await tabsOf('Statistics')).toEqual(['Overview', 'Planning', 'Goals']);
  });

  it('study modes open from the deck page; a running session gets the focus layout', async () => {
    const deck = (await api.deck.create('Focus')).id;
    await api.call('notes.create_many', {
      notetype_id: CLASSIC_NOTETYPE_ID,
      target: { kind: 'deck', id: deck },
      notes: ['cat:kedi', 'dog:köpek', 'bird:kuş', 'fish:balık'].map((p) => ({ fields: { front: p.split(':')[0]!, back: p.split(':')[1]! } })),
      tags: [],
      attributes: [],
      allow_duplicate: null,
    });
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: 'Library' }));
    const rows = await screen.findAllByTestId('deck-row');
    await fireEvent.click(rows.find((r) => r.textContent?.includes('Focus'))!);
    expect(await screen.findByTestId('deck-mode-cards')).toBeTruthy();
    // v1.2: the deck page (a sub-page with a back link) puts its title in the top bar's trail.
    const bar = screen.getByTestId('top-bar');
    await waitFor(() => expect(Array.from(bar.querySelectorAll('nav li'), (l) => l.textContent?.trim())).toEqual(['Library', 'Decks', 'Focus']));
    // Match has its tile next to the other modes.
    expect(await screen.findByTestId('deck-mode-match')).toBeTruthy();
    await fireEvent.click(await screen.findByTestId('deck-mode-test'));
    const page = document.querySelector('main .page')!;
    // v1.3: a mode button opens the study launcher with that mode chosen — never a silent start.
    await screen.findByTestId('study-launcher', {}, { timeout: 5_000 });
    // (The mode control appears once `study.options` has answered.)
    // v1.4: the paper (`test`) is called "Exam" ("Test" is the four-option quiz).
    expect((await screen.findByRole('radio', { name: 'Exam' }, { timeout: 5_000 })).getAttribute('aria-checked')).toBe('true');
    expect(page.classList.contains('focus')).toBe(false);
    const start = (await screen.findByTestId('launcher-start')) as HTMLButtonElement;
    await waitFor(() => expect(start.disabled).toBe(false));
    // v1.4: the footer says what Start does (mode · cards · count); the button is just "Start".
    expect(screen.getByTestId('launcher-summary').textContent).toMatch(/Exam · All cards · 4 questions/);
    await fireEvent.click(start);
    await waitFor(() => expect(page.classList.contains('focus')).toBe(true), { timeout: 5_000 });
    // The running paper owns the window: no deck picker, no Library tabs.
    expect(screen.queryByRole('combobox', { name: 'Deck' })).toBeNull();
    expect(screen.queryByRole('tablist', { name: 'Pages in Library' })).toBeNull();
    // Leaving the screen ends the focus layout; the sidebar goes back to the Library page.
    await fireEvent.click(screen.getByRole('button', { name: 'Library' }));
    await waitFor(() => expect(page.classList.contains('focus')).toBe(false));
    // (The Library reopens the deck's page: its back link, no section tabs on a sub-page.)
    expect(await screen.findByTestId('deck-mode-cards')).toBeTruthy();
    expect(screen.queryByRole('tablist', { name: 'Pages in Library' })).toBeNull();
  });

  it('mounts no screen behind the lock screen, lazy ones included', async () => {
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Profiles & lock' }));
    // A lazy screen (its own chunk) opened before locking.
    expect(await screen.findByText('Main profile', {}, { timeout: 5000 })).toBeTruthy();
    await api.call('lock.set', { kind: 'pin', secret: '4821', current: null, idle_minutes: null });
    await api.call('lock.lock', {});
    expect(await screen.findByRole('heading', { name: 'Flashcard is locked' })).toBeTruthy();
    await waitFor(() => expect(document.querySelector('main')?.textContent).not.toContain('Main profile'));
    // Switching to another (lazy) screen while locked loads nothing behind the lock.
    await fireEvent.click(screen.getByRole('button', { name: 'Library' }));
    expect(document.querySelector('main')?.textContent?.trim()).toBe('');
    await api.call('lock.unlock', { secret: '4821' });
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Flashcard is locked' })).toBeNull());
    await waitFor(() => expect(document.querySelector('main')?.textContent?.trim()).not.toBe(''));
  });

  it('v1.2 top bar: trail, search opens the palette, "Add card" and N open the Quick Add dialog', async () => {
    render(App);
    const bar = await screen.findByTestId('top-bar');
    const trail = () => Array.from(bar.querySelectorAll('nav li'), (l) => l.textContent?.trim());
    await waitFor(() => expect(trail()).toEqual(['Home']));
    await fireEvent.click(await screen.findByRole('button', { name: 'Statistics' }));
    await waitFor(() => expect(trail()).toEqual(['Statistics', 'Overview']));

    // The search field opens the command palette.
    await fireEvent.click(await screen.findByTestId('top-search'));
    expect(await screen.findByRole('dialog', { name: 'Command palette' })).toBeTruthy();
    await fireEvent.keyDown(window, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Command palette' })).toBeNull());

    // "Add card" opens the Quick Add dialog (v1.2; the page stays where it was).
    const nav = screen.getByRole('navigation', { name: 'Main navigation' });
    await fireEvent.click(await screen.findByRole('button', { name: 'Add card' }));
    expect(await screen.findByRole('dialog', { name: 'Add card' }, { timeout: 3000 })).toBeTruthy();
    expect(nav.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Statistics');
    await fireEvent.keyDown(document.activeElement ?? window, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Add card' })).toBeNull());

    // `N` does the same from anywhere — but not while typing in a field.
    await fireEvent.click(screen.getByRole('button', { name: 'Library' }));
    await waitFor(() => expect(nav.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Library'));
    const field = document.createElement('input');
    document.body.append(field);
    await fireEvent.keyDown(field, { key: 'n' });
    expect(screen.queryByRole('dialog', { name: 'Add card' })).toBeNull();
    field.remove();
    await fireEvent.keyDown(document.body, { key: 'n' });
    expect(await screen.findByRole('dialog', { name: 'Add card' })).toBeTruthy();
    expect(nav.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Library');
  });
});
