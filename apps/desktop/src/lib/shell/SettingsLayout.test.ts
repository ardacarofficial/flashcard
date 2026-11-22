import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import type { ModuleDto, UiContribution } from '$lib/api/types';
import { SCREENS, type ScreenDef } from '$lib/features';
import { settingsSearch } from '$lib/features/core/kit/search.svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { loadLocale } from '$lib/i18n/core';
import { buildNavigation } from './navigation';
import SettingsLayout from './SettingsLayout.svelte';

i18n.locale = 'en';

const HOME: ScreenDef = { id: 'home', load: async () => ({ default: (() => {}) as never }) };
const menu = (id: string, label_key: string, route: string): UiContribution => ({ slot: 'main_menu', id, label_key, icon: null, route, method: null, order: 50 });
const mod = (id: string, ui: UiContribution[]): ModuleDto => ({ id, name_key: '', description_key: '', enabled: true, can_disable: true, ui, methods: [] });

const nav = () =>
  buildNavigation(
    [
      mod('reminders', [menu('reminders', 'nav.reminders', '/reminders')]),
      mod('io', [menu('io', 'nav.io', '/io')]),
      mod('mcp', [menu('mcp', 'nav.mcp', '/mcp')]),
    ],
    SCREENS,
    HOME,
  );
const children = createRawSnippet(() => ({ render: () => '<p>page</p>' }));

describe('settings layout (v1.3)', () => {
  afterEach(() => {
    settingsSearch.query = '';
  });

  it('shows the groups with "Advanced" set apart', () => {
    render(SettingsLayout, { groups: nav().settings, active: null, open: () => {}, children });
    expect(screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent)).toEqual(['General', 'Data', 'Advanced']);
    expect(screen.getByText('Technical settings: you rarely need these.')).toBeTruthy();
  });

  it('search filters pages by what they contain and opens the chosen one', async () => {
    await loadLocale('en');
    const open = vi.fn();
    render(SettingsLayout, { groups: nav().settings, active: null, open, children });
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search settings' }), { target: { value: 'backup' } });
    const items = screen.getAllByTestId('subnav-item');
    expect(items.map((b) => b.querySelector('.name')?.textContent)).toEqual(['Import / export']);
    await fireEvent.click(items[0]!);
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ route: '/io' }));

    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search settings' }), { target: { value: 'qqqq' } });
    expect(screen.queryAllByTestId('subnav-item')).toHaveLength(0);
    expect(screen.getByText(/No setting mentions/)).toBeTruthy();
  });
});
