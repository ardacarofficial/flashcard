import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import { applyTheme, applyTokens, theme } from '$lib/theme/theme.svelte';
import ThemeSettings from './ThemeSettings.svelte';

i18n.locale = 'en';

describe('themes', () => {
  it('applies backend tokens as light-dark() CSS variables and removes them again', () => {
    const root = document.createElement('div');
    applyTokens({ light: { accent: '#2f6fdb', bg: '#ffffff' }, dark: { accent: '#6b9bf0', bg: '#161514' } }, root);
    expect(root.style.getPropertyValue('--fc-accent')).toBe('light-dark(#2f6fdb, #6b9bf0)');
    expect(root.style.getPropertyValue('--fc-bg')).toBe('light-dark(#ffffff, #161514)');
    applyTokens(null, root);
    expect(root.style.getPropertyValue('--fc-accent')).toBe('');
    applyTheme('dark', root);
    expect(root.dataset.theme).toBe('dark');
    applyTheme('system', root);
    expect(root.dataset.theme).toBeUndefined();
  });

  it('settings change scheme, preset and accent through themes.set and repaint', async () => {
    render(ThemeSettings);
    const sage = await screen.findByRole('radio', { name: /Sage/ });
    await fireEvent.click(sage);
    await fireEvent.change(sage);
    await expect.poll(() => theme.current?.preset).toBe('sage');
    expect(document.documentElement.style.getPropertyValue('--fc-accent')).toBe('light-dark(#3f7a5a, #7cc19a)');

    await fireEvent.change(screen.getByRole('combobox', { name: 'Colour scheme' }), { target: { value: 'dark' } });
    await expect.poll(() => document.documentElement.dataset.theme).toBe('dark');

    const picker = screen.getByLabelText('Use a custom accent');
    await fireEvent.change(picker, { target: { value: '#c2185b' } });
    await expect.poll(() => theme.current?.accent).toBe('#c2185b');
    expect(document.documentElement.style.getPropertyValue('--fc-accent')).toBe('light-dark(#c2185b, #c2185b)');
    await fireEvent.click(screen.getByRole('button', { name: 'Use the theme’s accent' }));
    await expect.poll(() => theme.current?.accent ?? null).toBe(null);
    expect((await api.call('themes.current', {})).preset).toBe('sage');
  });
});
