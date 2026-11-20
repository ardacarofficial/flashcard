import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
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
    expect(screen.getByText('On')).toBeTruthy();
    await fireEvent.click(name);
    expect(await screen.findByText('network:example.com')).toBeTruthy();
    expect(screen.getByText('asks separately')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Turn off' }));
    expect(await screen.findByText('Off')).toBeTruthy();
  });
});
