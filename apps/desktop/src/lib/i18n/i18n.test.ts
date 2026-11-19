import { describe, expect, it } from 'vitest';
import { DICTIONARIES, detectLocale, flatKeys, translate, translateDynamic } from './core';

describe('i18n', () => {
  it('TR and EN have exactly the same keys', () => {
    expect(flatKeys(DICTIONARIES.tr).sort()).toEqual(flatKeys(DICTIONARIES.en).sort());
  });

  it('no empty translations', () => {
    for (const dict of Object.values(DICTIONARIES)) {
      for (const k of flatKeys(dict)) expect(translateDynamic('en', k).length).toBeGreaterThan(0);
    }
  });

  it('looks up typed keys per locale', () => {
    expect(translate('tr', 'deck.new')).toBe('Yeni deste');
    expect(translate('en', 'deck.new')).toBe('New deck');
  });

  it('interpolates params', () => {
    expect(translate('en', 'deck.confirm_delete', { name: 'Verbs' })).toBe('Delete “Verbs” and its subdecks?');
  });

  it('dynamic keys fall back to the key itself', () => {
    expect(translateDynamic('tr', 'errors.validation.empty_name')).toBe('Ad boş olamaz.');
    expect(translateDynamic('tr', 'plugin.unknown.key')).toBe('plugin.unknown.key');
  });

  it('detects locale from browser languages', () => {
    expect(detectLocale(['en-US'])).toBe('en');
    expect(detectLocale(['de-DE', 'tr-TR'])).toBe('tr');
    expect(detectLocale(['fr'])).toBe('tr');
  });
});
