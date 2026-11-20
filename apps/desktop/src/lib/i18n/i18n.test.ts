import { afterEach, describe, expect, it, vi } from 'vitest';
import { DICTIONARIES, detectLocale, flatKeys, interpolate, isLocaleLoaded, loadLocale, translate, translateDynamic } from './core';
import { mergeLocales } from '../../../scripts/merge-locales.mjs';
import { i18n, initialLocale, setLocale } from './index.svelte';

describe('i18n', () => {
  it('per-feature files merge without conflicts into the generated dictionaries', () => {
    expect(mergeLocales()).toEqual({ en: DICTIONARIES.en, tr: DICTIONARIES.tr });
  });

  it('TR and EN have exactly the same keys', () => {
    expect(flatKeys(DICTIONARIES.tr).sort()).toEqual(flatKeys(DICTIONARIES.en).sort());
  });

  it('no empty translations', () => {
    for (const dict of Object.values(DICTIONARIES)) {
      for (const k of flatKeys(dict)) expect(translateDynamic('en', k).length).toBeGreaterThan(0);
    }
  });

  it('dictionaries are lazy chunks: loadLocale loads once and lookups use it', async () => {
    expect(isLocaleLoaded('en')).toBe(true); // tests/setup-i18n.ts
    const a = await loadLocale('tr');
    expect(await loadLocale('tr')).toBe(a);
    expect(DICTIONARIES.tr).toBe(a);
  });

  it('setLocale switches language after its dictionary loaded', async () => {
    const before = i18n.locale;
    await setLocale('en');
    expect(i18n.locale).toBe('en');
    expect(document.documentElement.lang).toBe('en');
    await setLocale(before);
    localStorage.removeItem('fc.locale');
  });

  it('looks up typed keys per locale', () => {
    expect(translate('tr', 'deck.new')).toBe('Yeni deste');
    expect(translate('en', 'deck.new')).toBe('New deck');
  });

  it('interpolates params', () => {
    expect(translate('en', 'deck.confirm_delete', { name: 'Verbs' })).toBe('Delete “Verbs” and its subdecks?');
  });

  it('picks ICU plural forms per locale', () => {
    const m = '{n, plural, =0 {none} one {# card} other {# cards}}';
    expect(interpolate(m, { n: 0 }, 'en')).toBe('none');
    expect(interpolate(m, { n: 1 }, 'en')).toBe('1 card');
    expect(interpolate(m, { n: 1234 }, 'en')).toBe('1,234 cards');
    expect(translate('en', 'create.paste.add', { count: 1 })).toBe('Add 1 card');
  });

  it('dynamic keys fall back to the key itself', () => {
    expect(translateDynamic('tr', 'errors.validation.deck_name')).toBe('Geçersiz deste adı.');
    expect(translateDynamic('en', 'nav.add_note')).toBe('Add');
    expect(translateDynamic('tr', 'modules.attributes.name')).toBe('Sınıflandırmalar');
    expect(translateDynamic('tr', 'plugin.unknown.key')).toBe('plugin.unknown.key');
  });

  it('detects locale from browser languages', () => {
    expect(detectLocale(['en-US'])).toBe('en');
    expect(detectLocale(['de-DE', 'tr-TR'])).toBe('tr');
    expect(detectLocale(['fr'])).toBe('tr');
  });

  describe('starting language (docs/11 §4)', () => {
    afterEach(() => {
      vi.unstubAllGlobals();
      localStorage.removeItem('fc.locale');
      history.replaceState(null, '', '/');
    });
    const system = (languages: string[]) =>
      vi.stubGlobal('navigator', { ...navigator, languages, language: languages[0] ?? '' });

    it('follows the system language when the user has not chosen one', () => {
      localStorage.removeItem('fc.locale');
      system(['tr-TR', 'en-US']);
      expect(initialLocale()).toBe('tr');
      system(['en-GB']);
      expect(initialLocale()).toBe('en');
    });

    it('a saved choice wins over the system; ?lang= (mock mode, screenshots) wins over both', () => {
      system(['tr-TR']);
      localStorage.setItem('fc.locale', 'en');
      expect(initialLocale()).toBe('en');
      history.replaceState(null, '', '/?lang=tr');
      expect(initialLocale()).toBe('tr');
      history.replaceState(null, '', '/?lang=xx');
      expect(initialLocale()).toBe('en');
    });
  });
});
