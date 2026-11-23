import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { api } from '$lib/api';
import { resetDefaultsCache } from '../core/kit/defaults.svelte';
import { describePermission } from './permissions';
import { i18n } from '$lib/i18n/index.svelte';
import { translateDynamic } from '$lib/i18n/core';
import PluginView from './PluginView.svelte';
import PluginsScreen from './PluginsScreen.svelte';
import { loadPluginTexts, parseCommandMethod, parseViewRoute, uiText } from './texts.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx: ScreenCtx = {
  decks: [],
  selectedDeck: null,
  selectedPath: null,
  selectDeck: () => {},
  guarded: async (fn) => {
    await fn();
    return true;
  },
  route: null,
};

describe('plugins (declarative UI on the dev mock)', () => {
  it('parses plugin routes and command methods', () => {
    expect(parseViewRoute('/plugins/view/com.example.reverse/reverse')).toEqual({
      pluginId: 'com.example.reverse',
      view: 'reverse',
    });
    expect(parseViewRoute('/plugins')).toBeNull();
    expect(parseCommandMethod('v1.plugin.com.example.autofill.filled-count')).toEqual({
      pluginId: 'com.example.autofill',
      command: 'filled-count',
    });
    expect(parseCommandMethod('v1.decks.create')).toBeNull();
  });

  it('shows plugin texts from the plugin locale files, literal data as is', async () => {
    await loadPluginTexts();
    expect(uiText('com.example.demo', { key: 'check', params: {} })).toBe('Check');
    expect(uiText('com.example.demo', { text: 'kılıç' })).toBe('kılıç');
    i18n.locale = 'tr';
    expect(uiText('com.example.demo', { key: 'check', params: {} })).toBe('Kontrol et');
    i18n.locale = 'en';
    // Plugin keys never leak into the app's own dictionaries.
    expect(translateDynamic('en', 'plugin.com.example.demo.check')).toBe('plugin.com.example.demo.check');
  });

  it('draws a study mode and sends actions with the input values', async () => {
    await loadPluginTexts();
    render(PluginView, { pluginId: 'com.example.demo', view: 'quiz' });
    expect(await screen.findByRole('heading', { name: 'sword' })).toBeTruthy();
    const input = (await screen.findByLabelText('Your answer')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'Kılıç' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Check' }));
    expect((await screen.findByRole('status')).textContent).toBe('Right!');
    expect(await screen.findByRole('heading', { name: 'shield' })).toBeTruthy();
  });

  it('lists installed plugins with their status and permissions', async () => {
    render(PluginsScreen, { ctx });
    const name = await screen.findByRole('button', { name: 'Demo' });
    // On/off is the switch alone (no "On" chip next to it).
    expect(screen.getByRole('switch', { name: 'Turn off' }).getAttribute('aria-checked')).toBe('true');
    expect(screen.queryByText('On')).toBeNull();
    await fireEvent.click(name);
    expect(await screen.findByText('network:example.com')).toBeTruthy();
    expect(screen.getByText('asks separately')).toBeTruthy();
    await fireEvent.click(screen.getByRole('switch', { name: 'Turn off' }));
    await waitFor(() => expect(screen.getByRole('switch', { name: 'Turn on' }).getAttribute('aria-checked')).toBe('false'));
  });

  it('says in plain words what each permission allows', async () => {
    expect(describePermission('network:example.com')).toBe('Can connect to example.com on the internet, and nowhere else.');
    expect(describePermission('notes:read')).toBe('Can read your notes (not change).');
    expect(describePermission('ui:study_mode')).toBe('Adds to the interface: a study mode.');
    expect(describePermission('fs:pick')).toBe('Can read a file, only one you choose.');
    expect(describePermission('hooks:before:notes.create')).toBe('Can step in before an action (notes.create) and change or stop it.');
    render(PluginsScreen, { ctx });
    expect(await screen.findByText('What is a plugin?')).toBeTruthy();
    await fireEvent.click(await screen.findByRole('button', { name: 'Demo' }));
    expect(await screen.findByText('Can connect to example.com on the internet, and nowhere else.')).toBeTruthy();
  });

  it('keeps the sandbox limits under a closed “Advanced”, marks defaults and resets them', async () => {
    localStorage.clear();
    resetDefaultsCache();
    const def = {
      key: 'plugins.timeout_ms',
      kind: { type: 'integer', min: 10, max: 60000 },
      default: 3000,
      label_key: 'settings.plugins.timeout_ms.label',
      description_key: 'settings.plugins.timeout_ms.description',
      advanced: true,
      scopes: ['app'],
    };
    let value = 5000;
    const calls: string[] = [];
    const real = api.call.bind(api);
    const spy = vi.spyOn(api, 'call').mockImplementation(((method: string, params: never) => {
      if (method === 'settings.schema') return Promise.resolve({ settings: [{ module: 'plugins', def }] });
      if (method === 'settings.get') return Promise.resolve({ values: [{ key: def.key, value, source: null }] });
      if (method === 'settings.reset') {
        calls.push(method);
        value = 3000;
        return Promise.resolve({ key: def.key, value, source: null });
      }
      return (real as (m: string, p: unknown) => Promise<unknown>)(method, params);
    }) as typeof api.call);
    try {
      render(PluginsScreen, { ctx });
      await screen.findByRole('button', { name: 'Demo' });
      expect(screen.queryByText('Time limit per call (ms)')).toBeNull();
      await fireEvent.click(screen.getByRole('button', { name: /More settings/ }));
      const limits = await screen.findByTestId('plugin-limits');
      expect(((await within(limits).findByLabelText('Time limit per call (ms)')) as HTMLInputElement).value).toBe('5000');
      await fireEvent.click(await within(limits).findByRole('button', { name: 'Reset to default' }));
      await waitFor(() => expect(calls).toEqual(['settings.reset']));
      expect(((await within(limits).findByLabelText('Time limit per call (ms)')) as HTMLInputElement).value).toBe('3000');
      expect(within(limits).getByText('default')).toBeTruthy();
    } finally {
      spy.mockRestore();
      resetDefaultsCache();
      localStorage.clear();
    }
  });
});
