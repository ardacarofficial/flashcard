import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
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
      'Study', 'Decks', 'Deck options', 'Add', 'Notes', 'Browse', 'Statistics', 'Planning', 'Goals', 'AI', 'Note types', 'Import / export', 'MCP', 'Automation', 'Maintenance', 'Plugins', 'Shortcuts', 'Settings',
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
});
