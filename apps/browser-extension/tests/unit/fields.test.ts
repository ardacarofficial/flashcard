import { beforeEach, describe, expect, it } from 'vitest';
import { collect, fieldsFor } from '../../src/lib/attributes';
import type { EffectiveAttributeDto } from '../../src/lib/contract';
import { setMessageLookup } from '../../src/lib/i18n';
import { readFields, renderFields } from '../../src/popup/fields';
import { MockApi, sampleData } from '../mock-api/handler';

beforeEach(() => setMessageLookup((k) => k));

function input(root: HTMLElement, selector: string): HTMLInputElement {
  const el = root.querySelector<HTMLInputElement>(selector);
  if (!el) throw new Error(`missing ${selector}`);
  return el;
}

describe('classification controls', () => {
  it('render the deck classifications pre-filled and read back the user choice', () => {
    const api = new MockApi();
    sampleData(api);
    const fields = fieldsFor(api.classifications['d-en'] ?? [], { 'attr-seviye': { type: 'choice', value: 'opt-A2' } });
    const root = document.createElement('div');
    renderFields(root, fields);
    const select = root.querySelector('select');
    if (!select) throw new Error('no select');
    expect(root.querySelector('label')?.textContent).toBe('Seviye');
    expect([...select.options].map((o) => o.textContent)).toEqual(['class_none', 'A1', 'A2', 'B1', 'B2', 'C1', 'C2']);
    expect(select.value).toBe('opt-A2');
    select.value = 'opt-B1';
    expect(collect(fields, readFields(root, fields)).attributes).toEqual({ 'attr-seviye': 'opt-B1' });
  });

  it('multi choice, bool, number, text, date', () => {
    const base = { scope_deck_id: null, target: 'note' as const, required: false, default_value: null, position: 0, template_id: null, updated_at: 0 };
    const opts = ['YDS', 'TOEFL'].map((l, i) => ({ id: `o-${l}`, label: l, color: null, order: i }));
    const list: EffectiveAttributeDto[] = [
      { def: { ...base, id: 'm', key: 'm', name: 'Sınav', kind: 'multi_enum', options: opts, required: true }, inherited: false, suggested_value: null },
      { def: { ...base, id: 'b', key: 'b', name: 'Zor', kind: 'bool', options: [] }, inherited: false, suggested_value: { type: 'bool', value: true } },
      { def: { ...base, id: 'n', key: 'n', name: 'Sayfa', kind: 'number', options: [] }, inherited: false, suggested_value: null },
      { def: { ...base, id: 't', key: 't', name: 'Not', kind: 'text', options: [] }, inherited: false, suggested_value: null },
      { def: { ...base, id: 'd', key: 'd', name: 'Tarih', kind: 'date', options: [] }, inherited: false, suggested_value: null },
    ];
    const fields = fieldsFor(list);
    const root = document.createElement('div');
    renderFields(root, fields);
    expect(root.querySelector('[data-attr="m"] label')?.textContent).toBe('Sınav *');
    expect(collect(fields, readFields(root, fields)).missing).toEqual(['Sınav']);
    const boxes = root.querySelectorAll<HTMLInputElement>('[data-attr="m"] input');
    const toefl = boxes[1];
    if (!toefl) throw new Error('no checkbox');
    toefl.checked = true;
    input(root, '[data-attr="n"] input').value = '42';
    input(root, '[data-attr="t"] input').value = 'ünite 3';
    input(root, '[data-attr="d"] input').value = '2026-11-20';
    const got = collect(fields, readFields(root, fields));
    expect(got.missing).toEqual([]);
    expect(got.attributes).toEqual({ m: ['o-TOEFL'], b: true, n: 42, t: 'ünite 3', d: '2026-11-20' });
  });
});
