import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { createRawSnippet, tick } from 'svelte';
import type { ModuleDto, UiContribution } from '$lib/api/types';
import { SCREENS, type ScreenDef } from '$lib/features';
import { settingsSearch } from '$lib/features/core/kit/search.svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { loadLocale } from '$lib/i18n/core';
import { buildNavigation } from './navigation';
import { advanced } from './advanced.svelte';
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

describe('settings layout (v1.4)', () => {
  afterEach(() => {
    settingsSearch.query = '';
    advanced.on = false;
  });

  it('shows Basic only; the one "Advanced" switch shows the other groups', async () => {
    await loadLocale('en');
    render(SettingsLayout, { groups: nav().settings, active: null, open: () => {}, children });
    expect(screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent)).toEqual(['Basics']);
    expect(screen.getByText(/1 more pages are hidden/)).toBeTruthy();
    advanced.on = true;
    await tick();
    expect(screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent)).toEqual(['Basics', 'Connections (details)']);
  });

  it('an Advanced page opened while the switch is off says so once and offers the switch', async () => {
    await loadLocale('en');
    const groups = nav().settings;
    const mcp = groups.flatMap((g) => g.entries).find((e) => e.route === '/mcp')!;
    render(SettingsLayout, { groups, active: mcp.id, open: () => {}, children });
    expect(screen.getByTestId('advanced-one-off').textContent).toContain('This page is in Advanced');
  });

  it('search filters pages by what they contain and opens the chosen one', async () => {
    await loadLocale('en');
    const open = vi.fn();
    render(SettingsLayout, { groups: nav().settings, active: null, open, children });
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search settings' }), { target: { value: 'backup' } });
    const items = screen.getAllByTestId('subnav-item');
    // v1.4: the page is Basic › "Backup".
    expect(items.map((b) => b.querySelector('.name')?.textContent)).toEqual(['Backup']);
    await fireEvent.click(items[0]!);
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ route: '/io' }));

    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search settings' }), { target: { value: 'qqqq' } });
    expect(screen.queryAllByTestId('subnav-item')).toHaveLength(0);
    expect(screen.getByText(/No setting mentions/)).toBeTruthy();
  });
});

describe('settings search finds Advanced pages with the switch off (v1.4)', () => {
  afterEach(() => {
    settingsSearch.query = '';
  });
  it('lists them marked "In Advanced"', async () => {
    await loadLocale('en');
    render(SettingsLayout, { groups: nav().settings, active: null, open: () => {}, children });
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search settings' }), { target: { value: 'assistant' } });
    const items = screen.getAllByTestId('subnav-item');
    expect(items.length).toBeGreaterThan(0);
    expect(items[0]!.querySelector('[data-testid="advanced-tag"]')?.textContent).toBe('In Advanced');
  });
});
