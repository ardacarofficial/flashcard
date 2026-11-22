import { describe, expect, it } from 'vitest';
import type { CardTemplate, FieldDef, NoteTypeInput } from '$lib/api/types';
import { asksForField, takesRole, withAnswerField, withRole, withoutField } from './roles';

const f = (key: string, extra: Partial<FieldDef> = {}): FieldDef => ({
  key,
  name: key,
  kind: 'rich_text',
  required: false,
  unique: false,
  searchable: true,
  default_value: null,
  ...extra,
});
const tpl = (key: string, extra: Partial<CardTemplate> = {}): CardTemplate => ({ key, name: key, front: '', back: '', card_type: 'classic', ...extra });

describe('note type editor: field roles and answer fields (draft shaping)', () => {
  it('a role belongs to one field: giving it to another moves it; none leaves the field as it came', () => {
    const fields = [f('word', { role: 'term' }), f('meaning', { role: 'meaning' }), f('extra')];
    const moved = withRole(fields, 2, 'meaning');
    expect(moved.map((x) => x.role ?? null)).toEqual(['term', null, 'meaning']);
    const cleared = withRole(moved, 2, null);
    expect(cleared.map((x) => x.role ?? null)).toEqual(['term', null, null]);
    // Unset = left out, as the backend sends it (so the draft compares equal again).
    expect('role' in cleared[2]!).toBe(false);
    expect(JSON.stringify(withRole(withRole(fields, 2, 'note'), 2, null))).toBe(JSON.stringify(fields));
    expect(fields[1]!.role).toBe('meaning');
  });

  it('a card asks for one field or (none) its whole back', () => {
    const templates = [tpl('recognition', { answer_field: 'meaning' }), tpl('production')];
    const set = withAnswerField(templates, 1, 'word');
    expect(set.map((x) => x.answer_field ?? null)).toEqual(['meaning', 'word']);
    const none = withAnswerField(set, 0, null);
    expect('answer_field' in none[0]!).toBe(false);
  });

  it('removing the field a card asks for: that card asks for its whole back again', () => {
    const def: NoteTypeInput = {
      name: 'x',
      kind: 'standard',
      fields: [f('word', { role: 'term' }), f('meaning', { role: 'meaning' })],
      templates: [tpl('a', { answer_field: 'meaning' }), tpl('b', { answer_field: 'word' })],
      css: '',
      sort_field: 'word',
    };
    const out = withoutField(def, 1);
    expect(out.fields.map((x) => x.key)).toEqual(['word']);
    expect(out.templates.map((x) => x.answer_field ?? null)).toEqual([null, 'word']);
  });

  it('text fields take a role; engines with their own answer have no answer field', () => {
    expect(takesRole(f('a', { kind: 'text' }))).toBe(true);
    expect(takesRole(f('a', { kind: 'image' }))).toBe(false);
    expect(asksForField(tpl('a'))).toBe(true);
    expect(asksForField(tpl('a', { card_type: 'cloze' }))).toBe(false);
    expect(asksForField(tpl('a', { card_type: 'multiple_choice' }))).toBe(false);
  });
});
