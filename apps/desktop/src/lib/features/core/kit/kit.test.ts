import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { loadLocale } from '$lib/i18n/core';
import KitFixture from './KitFixture.svelte';
import { matchesQuery, settingsSearch } from './search.svelte';
import { matchPage } from './settings-index';

i18n.locale = 'en';

const texts = {
  page: 'Page',
  row: 'New cards a day',
  desc: 'How many unseen cards a day.',
  example: '20 → at most 20 new words a day',
  rare: 'Maximum interval',
  rareDesc: 'Longest gap between two reviews.',
};

describe('settings kit (v1.3)', () => {
  beforeEach(() => {
    localStorage.clear();
    settingsSearch.query = '';
  });
  afterEach(() => {
    settingsSearch.query = '';
  });

  it('rows show a body-size description and a concrete example', () => {
    render(KitFixture, { texts });
    expect(screen.getByText('How many unseen cards a day.')).toBeTruthy();
    expect(screen.getByText('20 → at most 20 new words a day', { exact: false })).toBeTruthy();
  });

  it('"Advanced" is closed by default and remembers being opened', async () => {
    const first = render(KitFixture, { texts });
    const toggle = screen.getByRole('button', { name: /Advanced settings/ });
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByText('Maximum interval')).toBeNull();
    await fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByText('Maximum interval')).toBeTruthy();
    first.unmount();

    // Next visit: still open (this device), until closed again.
    render(KitFixture, { texts });
    const again = screen.getByRole('button', { name: /Advanced settings/ });
    expect(again.getAttribute('aria-expanded')).toBe('true');
    await fireEvent.click(again);
    expect(screen.queryByText('Maximum interval')).toBeNull();
    expect(localStorage.getItem('fc.settings.advanced.fixture')).toBeNull();
  });

  it('a search opens "Advanced" and lights up the matching row', () => {
    render(KitFixture, { texts });
    expect(screen.queryByText('Maximum interval')).toBeNull();
    settingsSearch.query = 'longest gap';
    flushSync();
    const row = screen.getByText('Maximum interval').closest('.fc-form-row');
    expect(row?.hasAttribute('data-search-hit')).toBe(true);
    expect(screen.getByText('New cards a day').closest('.fc-form-row')?.hasAttribute('data-search-hit')).toBe(false);
  });

  it('marks the default and offers a reset only when the value differs', async () => {
    const onReset = vi.fn();
    const r = render(KitFixture, { texts, isDefault: true, onReset });
    expect(screen.getByText('default')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Reset to default' })).toBeNull();
    r.unmount();
    render(KitFixture, { texts, isDefault: false, onReset });
    expect(screen.queryByText('default')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Reset to default' }));
    expect(onReset).toHaveBeenCalledOnce();
  });

  it('matches words case- and accent-insensitively, Turkish i included', () => {
    expect(matchesQuery('GÜNLÜK yeni', 'Günlük yeni kart sınırı')).toBe(true);
    expect(matchesQuery('isim', 'İSİM')).toBe(true);
    expect(matchesQuery('yeni kart', 'yeni', 'kelime')).toBe(false);
    expect(matchesQuery('', 'anything')).toBe(false);
  });

  it('finds a page by its name or by one text it shows (all words in that text)', async () => {
    await loadLocale('en');
    expect(matchPage('mcp', 'MCP', '/mcp', 'en')).toEqual({ snippet: null });
    const hit = matchPage('backup', 'Import / export', '/io', 'en');
    expect(hit?.snippet?.toLowerCase()).toContain('backup');
    expect(matchPage('zzzz qqqq', 'Import / export', '/io', 'en')).toBeNull();
  });
});
