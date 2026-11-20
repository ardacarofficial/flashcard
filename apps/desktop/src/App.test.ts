import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import App from './App.svelte';

i18n.locale = 'en';

describe('<App> on the dev mock transport', () => {
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
    expect(Array.from(tabs.querySelectorAll('[role=tab]'), (t) => t.textContent?.trim())).toEqual(['Add', 'AI', 'Import / export', 'Starter decks']);
    expect(nav.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Create');

    // v1.1 Create: a new deck or an existing one (design-system Select); classifications
    // follow the target deck — "Seviye" (A1–C2) is inherited from the parent deck.
    await fireEvent.click(await screen.findByRole('radio', { name: 'Existing deck' }));
    await fireEvent.click(await screen.findByRole('combobox', { name: 'Deck' }));
    await fireEvent.click(await screen.findByRole('option', { name: 'İngilizce::Fiiller' }));
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
    expect(groups).toEqual(['General', 'Study', 'Connections', 'Plugins', 'Data', 'Profiles and lock']);
    const pages = Array.from(sub.querySelectorAll('button'), (b) => b.textContent?.trim());
    expect(pages).toEqual(['General', 'Shortcuts', 'Reminders', 'Deck options', 'Note types', 'AI', 'MCP', 'Automation', 'Plugins', 'Import / export', 'Starter decks', 'Maintenance', 'Logs & updates', 'Profiles & lock']);
    await fireEvent.click(screen.getByRole('button', { name: 'Reminders' }));
    expect(sub.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('Reminders');

    // The other v1.0 screens live under the sections.
    const tabsOf = async (section: string) => {
      await fireEvent.click(screen.getByRole('button', { name: section }));
      const list = screen.queryByRole('tablist');
      return list ? Array.from(list.querySelectorAll('[role=tab]'), (t) => t.textContent?.trim()) : [];
    };
    expect(await tabsOf('Library')).toEqual(['Decks', 'Notes', 'Study', 'Learn', 'Test', 'Match', 'Reading']);
    expect(await tabsOf('Search')).toEqual([]);
    expect(await tabsOf('Statistics')).toEqual(['Statistics', 'Planning', 'Goals']);
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
});
