import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import App from './App.svelte';

i18n.locale = 'en';

describe('<App> on the dev mock transport', () => {
  it('draws the menu from modules.list and shows classification fields', async () => {
    render(App);
    // Menu items come from modules.list main_menu contributions (+ Settings for settings pages).
    await screen.findByRole('button', { name: 'Settings' });
    const nav = screen.getByRole('navigation');
    expect(Array.from(nav.querySelectorAll('button'), (b) => b.textContent?.trim())).toEqual([
      'Study', 'Learn', 'Test', 'Decks', 'Deck options', 'Add', 'Inbox', 'Notes', 'Browse', 'Statistics', 'Planning', 'Goals', 'AI', 'Reading', 'Note types', 'Import / export', 'MCP', 'Automation', 'Maintenance', 'Plugins', 'Starter decks', 'Shortcuts', 'Logs & updates', 'Reminders', 'Profiles & lock', 'Settings',
    ]);
    await fireEvent.click(await screen.findByRole('button', { name: 'Add' }));
    const deck = (await screen.findByLabelText('Deck')) as HTMLSelectElement;
    await screen.findByRole('option', { name: 'İngilizce::Fiiller' });
    const sub = Array.from(deck.options).find((o) => o.textContent === 'İngilizce::Fiiller')!;
    await fireEvent.change(deck, { target: { value: sub.value } });
    // Inherited "Seviye" (A1–C2) from the parent deck.
    const level = (await screen.findByLabelText('Seviye')) as HTMLSelectElement;
    expect(Array.from(level.options).map((o) => o.textContent)).toEqual([
      'Not set', 'A1', 'A2', 'B1', 'B2', 'C1', 'C2',
    ]);
  });

  it('mounts no screen behind the lock screen, lazy ones included', async () => {
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: 'Profiles & lock' }));
    // A lazy screen (its own chunk) opened before locking.
    expect(await screen.findByText('Main profile', {}, { timeout: 5000 })).toBeTruthy();
    await api.call('lock.set', { kind: 'pin', secret: '4821', current: null, idle_minutes: null });
    await api.call('lock.lock', {});
    expect(await screen.findByRole('heading', { name: 'Flashcard is locked' })).toBeTruthy();
    await waitFor(() => expect(document.querySelector('main')?.textContent).not.toContain('Main profile'));
    // Switching to another (lazy) screen while locked loads nothing behind the lock.
    await fireEvent.click(screen.getByRole('button', { name: 'Starter decks' }));
    expect(document.querySelector('main')?.textContent?.trim()).toBe('');
    await api.call('lock.unlock', { secret: '4821' });
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Flashcard is locked' })).toBeNull());
    await waitFor(() => expect(document.querySelector('main')?.textContent?.trim()).not.toBe(''));
  });
});
