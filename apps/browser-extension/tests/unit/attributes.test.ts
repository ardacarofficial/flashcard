import { describe, expect, it } from 'vitest';
import { collect, fieldsFor, parseRaw } from '../../src/lib/attributes';
import type { EffectiveAttributeDto } from '../../src/lib/contract';
import { MockApi, sampleData } from '../mock-api/handler';

function sample(): EffectiveAttributeDto[] {
  const api = new MockApi();
  sampleData(api);
  const seviye = api.classifications['d-en']![0]!;
  const def = (over: Partial<EffectiveAttributeDto['def']>): EffectiveAttributeDto => ({ def: { ...seviye.def, options: [], ...over }, inherited: false, suggested_value: null });
  return [
    // options deliberately out of order: the form must show the user's order
    { ...seviye, def: { ...seviye.def, options: [...seviye.def.options].reverse() }, suggested_value: { type: 'choice', value: 'opt-A2' } },
    def({ id: 'attr-sinav', key: 'sinav', name: 'Sınav', kind: 'multi_enum', required: true, options: ['YDS', 'TOEFL'].map((l, i) => ({ id: `o-${l}`, label: l, color: null, order: i })) }),
    def({ id: 'attr-sayfa', key: 'sayfa', name: 'Sayfa', kind: 'number' }),
    def({ id: 'attr-not', key: 'not', name: 'Not', kind: 'text', default_value: { type: 'text', value: 'web' } }),
    def({ id: 'attr-tarih', key: 'tarih', name: 'Tarih', kind: 'date' }),
    def({ id: 'attr-zor', key: 'zor', name: 'Zor', kind: 'bool' }),
    def({ id: 'attr-kart', key: 'kart', name: 'Kart', kind: 'text', target: 'card' }),
  ];
}

describe('fieldsFor', () => {
  it('keeps note classifications in order with sorted options and the suggestion/default pre-filled', () => {
    const f = fieldsFor(sample());
    expect(f.map((x) => x.key)).toEqual(['seviye', 'sinav', 'sayfa', 'not', 'tarih', 'zor']);
    expect(f[0]!.options.map((o) => o.label)).toEqual(['A1', 'A2', 'B1', 'B2', 'C1', 'C2']);
    expect(f[0]!.initial).toEqual({ type: 'choice', value: 'opt-A2' });
    expect(f[3]!.initial).toEqual({ type: 'text', value: 'web' });
  });

  it('prefers the last used value, including an explicit empty one, and drops stale options', () => {
    expect(fieldsFor(sample(), { 'attr-seviye': { type: 'choice', value: 'opt-B1' } })[0]!.initial).toEqual({ type: 'choice', value: 'opt-B1' });
    expect(fieldsFor(sample(), { 'attr-seviye': null })[0]!.initial).toBeNull();
    expect(fieldsFor(sample(), { 'attr-seviye': { type: 'choice', value: 'deleted-option' } })[0]!.initial).toBeNull();
  });
});

describe('collect', () => {
  it('builds the inbox attributes map keyed by attribute id with option ids', () => {
    const f = fieldsFor(sample());
    const got = collect(f, {
      'attr-seviye': 'opt-B1',
      'attr-sinav': ['o-YDS', 'o-TOEFL'],
      'attr-sayfa': '12,5',
      'attr-not': '  ',
      'attr-tarih': '2026-11-20',
      'attr-zor': false,
    });
    expect(got.missing).toEqual([]);
    expect(got.invalid).toEqual([]);
    expect(got.attributes).toEqual({ 'attr-seviye': 'opt-B1', 'attr-sinav': ['o-YDS', 'o-TOEFL'], 'attr-sayfa': 12.5, 'attr-tarih': '2026-11-20', 'attr-zor': false });
    expect(got.remember['attr-not']).toBeNull();
  });

  it('reports required and invalid fields', () => {
    const f = fieldsFor(sample());
    const got = collect(f, { 'attr-sinav': [], 'attr-sayfa': 'abc', 'attr-tarih': '20.11.2026' });
    expect(got.missing).toEqual(['Sınav']);
    expect(got.invalid).toEqual(['Sayfa', 'Tarih']);
  });

  it('parses each kind', () => {
    expect(parseRaw('enum', '')).toBeNull();
    expect(parseRaw('bool', null)).toBeNull();
    expect(parseRaw('bool', true)).toEqual({ type: 'bool', value: true });
    expect(parseRaw('number', ' 3 ')).toEqual({ type: 'number', value: 3 });
  });
});
