import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import type { AttributeDefDto, AttributeKind } from '$lib/api/types';
import {
  controlFor,
  displayOptions,
  fromInput,
  initialValue,
  toAssignments,
  toInput,
} from './attribute-field';
import AttributeField from './AttributeField.svelte';
import { i18n } from '$lib/i18n/index.svelte';

i18n.locale = 'tr';

function def(kind: AttributeKind, extra: Partial<AttributeDefDto> = {}): AttributeDefDto {
  return {
    id: `a-${kind}`,
    scope_deck_id: null,
    key: kind,
    name: `Attr ${kind}`,
    kind,
    options: [
      { id: 'o2', label: 'B1', color: null, order: 2 },
      { id: 'o1', label: 'A1', color: null, order: 1 },
    ],
    target: 'note',
    required: false,
    default_value: null,
    position: 0,
    template_id: null,
    updated_at: 0,
    ...extra,
  };
}

describe('attribute field logic', () => {
  it('maps every kind to a control', () => {
    expect(controlFor('ordered_enum')).toBe('select');
    expect(controlFor('enum')).toBe('select');
    expect(controlFor('multi_enum')).toBe('multiselect');
    expect(controlFor('number')).toBe('number');
    expect(controlFor('text')).toBe('text');
    expect(controlFor('date')).toBe('date');
    expect(controlFor('bool')).toBe('tristate');
  });

  it('orders options by backend order', () => {
    expect(displayOptions(def('ordered_enum')).map((o) => o.label)).toEqual(['A1', 'B1']);
  });

  it('initial values honour defaults and kind', () => {
    // Untouched fields stay null: Rust applies defaults, not the frontend.
    expect(initialValue(def('multi_enum'))).toBeNull();
    expect(initialValue(def('bool'))).toBeNull();
    expect(initialValue(def('text'))).toBeNull();
    expect(initialValue(def('enum', { default_value: { type: 'choice', value: 'o1' } }))).toEqual({
      type: 'choice',
      value: 'o1',
    });
  });

  it('converts raw input', () => {
    expect(fromInput('number', '12.5')).toEqual({ type: 'number', value: 12.5 });
    expect(fromInput('number', '')).toBeNull();
    expect(fromInput('enum', '')).toBeNull();
    expect(fromInput('ordered_enum', 'o1')).toEqual({ type: 'choice', value: 'o1' });
    expect(fromInput('multi_enum', ['o1'])).toEqual({ type: 'choices', value: ['o1'] });
    expect(fromInput('multi_enum', [])).toBeNull();
    expect(fromInput('bool', 'true')).toEqual({ type: 'bool', value: true });
    expect(fromInput('bool', 'false')).toEqual({ type: 'bool', value: false });
    expect(fromInput('bool', '')).toBeNull();
    expect(fromInput('date', '2026-01-02')).toEqual({ type: 'date', value: '2026-01-02' });
    expect(fromInput('text', 'x')).toEqual({ type: 'text', value: 'x' });
  });

  it('round-trips single values to input state', () => {
    expect(toInput(null)).toBe('');
    expect(toInput({ type: 'bool', value: false })).toBe('false');
    expect(toInput({ type: 'number', value: 7 })).toBe('7');
    expect(toInput({ type: 'choice', value: 'o1' })).toBe('o1');
  });

  it('sends only touched fields as assignments', () => {
    expect(
      toAssignments({ a: null, b: { type: 'bool', value: false }, c: null }),
    ).toEqual([{ attr_id: 'b', value: { type: 'bool', value: false } }]);
  });
});

describe('<AttributeField>', () => {
  it('renders ordered_enum as a select with an unset option', async () => {
    render(AttributeField, { def: def('ordered_enum'), value: null });
    const select = screen.getByLabelText('Attr ordered_enum') as HTMLSelectElement;
    expect(select.tagName).toBe('SELECT');
    expect(select.multiple).toBe(false);
    expect(Array.from(select.options).map((o) => o.textContent)).toEqual(['Seçilmedi', 'A1', 'B1']);
  });

  it('renders multi_enum as a multi-select', () => {
    render(AttributeField, { def: def('multi_enum'), value: { type: 'choices', value: ['o2'] } });
    const select = screen.getByLabelText('Attr multi_enum') as HTMLSelectElement;
    expect(select.multiple).toBe(true);
    expect(Array.from(select.selectedOptions).map((o) => o.value)).toEqual(['o2']);
  });

  it.each([
    ['number', 'number'],
    ['text', 'text'],
    ['date', 'date'],
  ] as const)('renders %s as input[type=%s]', (kind, type) => {
    render(AttributeField, { def: def(kind), value: initialValue(def(kind)) });
    expect((screen.getByLabelText(`Attr ${kind}`) as HTMLInputElement).type).toBe(type);
  });

  it('renders bool as a three-state select', () => {
    render(AttributeField, { def: def('bool'), value: null });
    const select = screen.getByLabelText('Attr bool') as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.value)).toEqual(['', 'true', 'false']);
    expect(select.value).toBe('');
  });

  it('marks required fields without native validation', () => {
    render(AttributeField, { def: def('text', { required: true }), value: null });
    expect((screen.getByLabelText(/Attr text/) as HTMLInputElement).required).toBe(false);
    expect(screen.getByTitle('Zorunlu')).toBeTruthy();
  });

  it('reports changes back as typed values', async () => {
    let value: unknown = null;
    render(AttributeField, {
      props: {
        def: def('number'),
        get value() {
          return null;
        },
        set value(v) {
          value = v;
        },
      },
    });
    await fireEvent.input(screen.getByLabelText('Attr number'), { target: { value: '7' } });
    expect(value).toEqual({ type: 'number', value: 7 });
  });
});
