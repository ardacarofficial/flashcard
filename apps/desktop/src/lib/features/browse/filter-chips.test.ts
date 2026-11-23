import { describe, expect, it } from 'vitest';
import { chipDefs, findToken, fold, removeToken, setToken, tokens, valueOf } from './filter-chips';

const STATUS = ['durum', 'status'];
const SEEN = ['karsilasma', 'seen'];

describe('filter chips: query tokens', () => {
  it('folds like the Rust parser', () => {
    expect(fold('Karşılaşma')).toBe('karsilasma');
    expect(fold('SIKLIK')).toBe('siklik');
    expect(fold('İçe_Aktarma')).toBe('ice_aktarma');
    expect(fold('Öğreniyorum')).toBe('ogreniyorum');
  });

  it('splits tokens, keeping quoted values together', () => {
    const t = tokens('deck:İngilizce  kaynak:"Night Train" -ses:var kedi');
    expect(t.map((x) => x.text)).toEqual(['deck:İngilizce', 'kaynak:"Night Train"', '-ses:var', 'kedi']);
    expect(t[1]).toMatchObject({ key: 'kaynak', value: 'Night Train', negated: false });
    expect(t[2]).toMatchObject({ key: 'ses', negated: true });
    expect(t[3]!.key).toBeNull();
    expect(tokens('karşılaşma>=2')[0]).toMatchObject({ key: 'karsilasma', value: '2' });
  });

  it('appends a token to the query', () => {
    expect(setToken('', STATUS, 'durum:yeni')).toBe('durum:yeni');
    expect(setToken('  kedi ', STATUS, 'durum:yeni')).toBe('kedi durum:yeni');
  });

  it('replaces the token in place, by any alias of its key', () => {
    expect(setToken('kedi durum:yeni deck:A', STATUS, 'durum:biliyorum')).toBe('kedi durum:biliyorum deck:A');
    expect(setToken('Status:new kedi', STATUS, 'durum:yeni')).toBe('durum:yeni kedi');
    expect(setToken('seen>=3 x karşılaşma=1', SEEN, 'karşılaşma>=2')).toBe('karşılaşma>=2 x');
  });

  it('leaves negated tokens and other keys alone', () => {
    expect(setToken('-durum:yeni', STATUS, 'durum:biliyorum')).toBe('-durum:yeni durum:biliyorum');
    expect(findToken('-durum:yeni', STATUS)).toBeNull();
    expect(setToken('dil:en', STATUS, 'durum:yeni')).toBe('dil:en durum:yeni');
  });

  it('removes the token and tidies spaces', () => {
    expect(removeToken('kedi durum:yeni deck:A', STATUS)).toBe('kedi deck:A');
    expect(removeToken('durum:yeni', STATUS)).toBe('');
    expect(removeToken('kedi  köpek', STATUS)).toBe('kedi  köpek');
  });

  it('knows which menu value a token stands for', () => {
    const from = chipDefs().find((c) => c.id === 'from')!;
    const tok = findToken('nereden:Altyazı', from.keys)!;
    expect(valueOf(from, tok)?.token).toBe('nereden:altyazı');
    expect(valueOf(from, findToken('nereden:subtitle', from.keys)!)).toBeNull();
  });

  it('offers the languages the collection has, plus "no language"', () => {
    const lang = chipDefs('en', ['de', 'en']).find((c) => c.id === 'lang')!;
    expect(lang.values.map((v) => v.token)).toEqual(['dil:de', 'dil:en', 'dil:yok']);
    expect(chipDefs('en').find((c) => c.id === 'lang')!.values.map((v) => v.token)).toEqual(['dil:yok']);
  });

  it('every chip value writes a token of its own key', () => {
    for (const def of chipDefs('tr', ['en', 'de'])) {
      for (const v of def.values) expect(def.keys).toContain(tokens(v.token)[0]!.key);
      if (def.text) expect(def.keys).toContain(fold(def.text.slice(0, -1)));
    }
  });
});
