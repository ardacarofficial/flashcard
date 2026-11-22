import { describe, expect, it } from 'vitest';
import type { FieldDef, NoteTypeDto } from '$lib/api/types';
import { decodeColumn, encodeColumn, prefillMapping, resolveColumn } from './mapping';
import { fieldRole, fillOrder, primaryFields } from '../notes/roles';

const f = (key: string, name: string, extra: Partial<FieldDef> = {}): FieldDef => ({
  key,
  name,
  kind: 'rich_text',
  required: false,
  unique: false,
  searchable: true,
  default_value: null,
  ...extra,
});
const type = (builtin: string | null, fields: FieldDef[]): NoteTypeDto => ({
  id: `nt-${builtin ?? 'custom'}`,
  name: builtin ?? 'Custom',
  kind: 'standard',
  fields,
  templates: [],
  css: '',
  sort_field: fields[0]!.key,
  version: 1,
  builtin,
  updated_at: 0,
});

// Roles as the backend sends them for the built-ins (v1.3 migration).
const classic = type('classic', [f('front', 'Front', { required: true, unique: true, role: 'term' }), f('back', 'Back', { role: 'meaning' })]);
const vocabulary = type('vocabulary', [
  f('word', 'Word', { unique: true, role: 'term' }),
  f('meaning', 'Meaning', { required: true, role: 'meaning' }),
  f('pronunciation', 'Pronunciation', { kind: 'text' }),
  f('example', 'Example sentence', { role: 'example' }),
  f('audio', 'Audio', { kind: 'audio' }),
]);
const subtitle = type('subtitle', [
  f('word', 'Word', { required: true, role: 'term' }),
  f('surface', 'As it appeared', { kind: 'text', role: 'surface' }),
  f('pos', 'Part of speech', { kind: 'text' }),
  f('meaning', 'Meaning', { role: 'meaning' }),
  f('sentence', 'Sentence', { role: 'example' }),
  f('translation', 'Translation', { role: 'example_translation' }),
  f('image', 'Screenshot', { kind: 'image' }),
]);
const custom = type(null, [f('q', 'Question'), f('a', 'Answer'), f('hint', 'Hint')]);

const keys = (cols: { kind: string; key?: string }[]) => cols.map((c) => (c.kind === 'field' ? c.key : c.kind));

describe('field roles (display only; the backend owns them)', () => {
  it("reads the backend's roles, never guessing from keys or labels", () => {
    expect(vocabulary.fields.map((x) => fieldRole(vocabulary, x))).toEqual(['term', 'meaning', null, 'example', null]);
    expect(fieldRole(subtitle, subtitle.fields[4]!)).toBe('example');
    expect(custom.fields.map((x) => fieldRole(custom, x))).toEqual([null, null, null]);
    // A user type that merely calls a field "Meaning" gets no role from its label.
    expect(fieldRole(type(null, [f('x', 'Meaning')]), f('x', 'Meaning'))).toBeNull();
    // v1.3 (deliberate change): a built-in whose role the user cleared has none — the stable
    // key `sentence` no longer stands in for the role.
    const cleared = type('subtitle', [f('word', 'Word', { role: 'term' }), f('sentence', 'Sentence')]);
    expect(fieldRole(cleared, cleared.fields[1]!)).toBeNull();
  });

  it("prefers the backend's role when the field carries one (v1.3 API)", () => {
    const sent = { ...f('q', 'Question'), role: 'example' } as FieldDef;
    expect(fieldRole(custom, sent)).toBe('example');
    const none = { ...f('word', 'Word'), role: null } as unknown as FieldDef;
    expect(fieldRole(vocabulary, none)).toBeNull();
  });

  it('fills text fields in role order: term, meaning, example; then the rest', () => {
    expect(fillOrder(vocabulary).map((x) => x.key)).toEqual(['word', 'meaning', 'example', 'pronunciation']);
    expect(fillOrder(subtitle).map((x) => x.key)).toEqual(['word', 'meaning', 'sentence', 'translation', 'surface', 'pos']);
    expect(fillOrder(custom).map((x) => x.key)).toEqual(['q', 'a', 'hint']);
  });

  it('Quick Add shows term, meaning and example up front; types without roles show everything', () => {
    expect(primaryFields(vocabulary).map((x) => x.key)).toEqual(['word', 'meaning', 'example']);
    expect(primaryFields(subtitle).map((x) => x.key)).toEqual(['word', 'meaning', 'sentence']);
    expect(primaryFields(classic).map((x) => x.key)).toEqual(['front', 'back']);
    expect(primaryFields(custom)).toEqual(custom.fields);
  });
});

describe('role column targets (io.import accepts role names)', () => {
  it('round-trip through the picker value and resolve to the field holding the role', () => {
    expect(decodeColumn(encodeColumn({ kind: 'role', role: 'example' }))).toEqual({ kind: 'role', role: 'example' });
    expect(decodeColumn('role:nonsense')).toEqual({ kind: 'ignore' });
    expect(resolveColumn({ kind: 'role', role: 'example' }, subtitle)).toEqual({ kind: 'field', key: 'sentence' });
    expect(resolveColumn({ kind: 'role', role: 'example' }, classic)).toEqual({ kind: 'ignore' });
    expect(resolveColumn({ kind: 'tags' }, classic)).toEqual({ kind: 'tags' });
  });

  it('a field keyed "example" without the role is not the example column', () => {
    const plain = type(null, [f('word', 'Word', { role: 'term' }), f('example', 'Example'), f('ctx', 'Context', { role: 'example' })]);
    expect(fillOrder(plain).map((x) => x.key)).toEqual(['word', 'ctx', 'example']);
    expect(resolveColumn({ kind: 'role', role: 'example' }, plain)).toEqual({ kind: 'field', key: 'ctx' });
  });
});

describe('mapping pre-fill', () => {
  const rows = [
    ['go', 'gitmek', 'I go to school by bus.'],
    ['book', 'kitap', 'This book is about a small village.'],
  ];

  it('word · meaning · sentence fill a vocabulary type by role (not pronunciation)', () => {
    const m = prefillMapping(rows, 3, vocabulary, null);
    expect(keys(m.columns)).toEqual(['word', 'meaning', 'example']);
    expect(m).toMatchObject({ header: false, origin: 'order' });
  });

  it('a third column has nowhere to go in Classic: it is not imported (never doubled)', () => {
    expect(keys(prefillMapping(rows, 3, classic, null).columns)).toEqual(['front', 'back', 'ignore']);
  });

  it('a header row maps by role words (EN/TR), field names and tags, and is skipped', () => {
    const tr = prefillMapping([['Kelime', 'Anlam', 'Örnek cümle', 'Etiketler'], ...rows], 4, vocabulary, null);
    expect(keys(tr.columns)).toEqual(['word', 'meaning', 'example', 'tags']);
    expect(tr).toMatchObject({ header: true, origin: 'header' });
    const en = prefillMapping([['sentence', 'definition', 'term'], ...rows], 3, classic, null);
    // Classic has no example field: "sentence" is not imported; the others find their fields.
    expect(keys(en.columns)).toEqual(['ignore', 'back', 'front']);
    const names = prefillMapping([['Question', 'Hint', 'Answer'], ...rows], 3, custom, null);
    expect(keys(names.columns)).toEqual(['q', 'hint', 'a']);
  });

  it('one stray word in a data row is not a header', () => {
    const m = prefillMapping([['note', 'not', 'a short message'], ...rows], 3, classic, null);
    expect(m.header).toBe(false);
  });

  it('remembers the last mapping per note type (same width; same header wins over guesses)', () => {
    const saved = { header: null, columns: ['field:meaning', 'field:word', 'ignore'] };
    expect(keys(prefillMapping(rows, 3, vocabulary, saved).columns)).toEqual(['meaning', 'word', 'ignore']);
    expect(prefillMapping(rows, 3, vocabulary, saved).origin).toBe('remembered');
    // Another width, or a field the type no longer has: back to the role order.
    expect(prefillMapping(rows, 2, vocabulary, saved).origin).toBe('order');
    expect(prefillMapping(rows, 3, vocabulary, { header: null, columns: ['field:gone', 'ignore', 'ignore'] }).origin).toBe('order');
    const headed = { header: ['A', 'B', 'C'], columns: ['field:example', 'field:word', 'field:meaning'] };
    const m = prefillMapping([['a', 'b', 'c'], ...rows], 3, vocabulary, headed);
    expect(keys(m.columns)).toEqual(['example', 'word', 'meaning']);
    expect(m).toMatchObject({ header: true, origin: 'remembered' });
  });
});
