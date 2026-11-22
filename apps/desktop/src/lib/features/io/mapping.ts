import type { ColumnTarget, NoteTypeDto } from '$lib/api/types';
import { fieldRole, fillOrder, isTextField, type Role } from '../notes/roles';

/** `<select>` value of a column target. */
export function encodeColumn(c: ColumnTarget): string {
  switch (c.kind) {
    case 'field':
      return `field:${c.key}`;
    case 'attribute':
      return `attr:${c.attr_id}`;
    default:
      return c.kind;
  }
}

/** Inverse of {@link encodeColumn}. */
export function decodeColumn(v: string): ColumnTarget {
  if (v.startsWith('field:')) return { kind: 'field', key: v.slice(6) };
  if (v.startsWith('attr:')) return { kind: 'attribute', attr_id: v.slice(5) };
  if (v === 'tags') return { kind: 'tags' };
  return { kind: 'ignore' };
}

/** Separator as typed in an input: a tab is written `\t`. */
export function escapeSeparator(s: string): string {
  return s.replaceAll('\t', '\\t');
}

/** Inverse of {@link escapeSeparator}. */
export function unescapeSeparator(s: string): string {
  return s.replaceAll('\\t', '\t');
}

// ---- v1.3: pre-filled column mapping --------------------------------------------------------

/**
 * Header words per role (EN + TR, lower case). A first row made of such words is a header:
 * its columns map to the fields with those roles. Field keys and names always count too.
 */
const ROLE_WORDS: Record<Role, string[]> = {
  term: ['term', 'word', 'words', 'front', 'question', 'expression', 'english', 'kelime', 'sözcük', 'terim', 'ön', 'ön yüz', 'soru', 'ingilizce'],
  surface: ['surface', 'form', 'as it appeared', 'geçtiği hâli', 'geçtiği hali', 'biçim'],
  meaning: ['meaning', 'definition', 'back', 'answer', 'turkish', 'anlam', 'anlamı', 'tanım', 'arka', 'arka yüz', 'cevap', 'türkçe', 'karşılık'],
  example: ['example', 'sentence', 'example sentence', 'context', 'usage', 'örnek', 'cümle', 'örnek cümle', 'bağlam', 'kullanım'],
  example_translation: ['translation', 'sentence translation', 'example translation', 'çeviri', 'cümle çevirisi', 'örnek çevirisi'],
  source: ['source', 'from', 'kaynak', 'nereden'],
  note: ['note', 'notes', 'comment', 'not', 'notlar', 'açıklama'],
};
const TAG_WORDS = ['tags', 'tag', 'etiketler', 'etiket'];

const norm = (s: string) => s.trim().toLocaleLowerCase('tr').replace(/\s+/g, ' ');

/** What a header cell names in `nt` (`null`: nothing it knows). */
export function headerTarget(cell: string, nt: NoteTypeDto | null, attributes: { id: string; key: string; name: string }[] = []): ColumnTarget | null {
  const c = norm(cell);
  if (!c || !nt) return null;
  const text = nt.fields.filter(isTextField);
  const byName = text.find((f) => norm(f.key) === c || norm(f.name) === c);
  if (byName) return { kind: 'field', key: byName.key };
  for (const f of text) {
    const role = fieldRole(nt, f);
    if (role && ROLE_WORDS[role].includes(c)) return { kind: 'field', key: f.key };
  }
  if (TAG_WORDS.includes(c)) return { kind: 'tags' };
  const attr = attributes.find((a) => norm(a.key) === c || norm(a.name) === c);
  return attr ? { kind: 'attribute', attr_id: attr.id } : null;
}

/** A remembered mapping (per note type): the header it was made for and the targets. */
export interface SavedMapping {
  header: string[] | null;
  columns: string[];
}

const SAVED = (ntId: string) => `fc.import.mapping.${ntId}`;

export function loadMapping(ntId: string): SavedMapping | null {
  try {
    const raw = localStorage.getItem(SAVED(ntId));
    const v = raw ? (JSON.parse(raw) as SavedMapping) : null;
    return v && Array.isArray(v.columns) ? v : null;
  } catch {
    return null;
  }
}

export function saveMapping(ntId: string, mapping: SavedMapping): void {
  try {
    localStorage.setItem(SAVED(ntId), JSON.stringify(mapping));
  } catch {
    /* ignore */
  }
}

/** Where the mapping came from (the UI says it in one line). */
export type MappingOrigin = 'header' | 'remembered' | 'order';

/**
 * The mapping a preview starts with:
 * 1. a header row (cells naming fields, roles, tags or classifications) maps its columns and
 *    is skipped;
 * 2. else the mapping last used with this note type, when it fits the column count;
 * 3. else columns fill the fields in role order (term, meaning, example, …).
 * A field is never the target of two columns; extra columns are not imported.
 */
export function prefillMapping(
  rows: string[][],
  columns: number,
  nt: NoteTypeDto | null,
  saved: SavedMapping | null,
  attributes: { id: string; key: string; name: string }[] = [],
): { columns: ColumnTarget[]; header: boolean; origin: MappingOrigin } {
  const first = rows[0] ?? [];
  const used = new Set<string>();
  const once = (c: ColumnTarget | null): ColumnTarget => {
    if (!c) return { kind: 'ignore' };
    const k = encodeColumn(c);
    if (c.kind !== 'tags' && used.has(k)) return { kind: 'ignore' };
    used.add(k);
    return c;
  };
  const known = new Set(fillOrder(nt).map((f) => `field:${f.key}`));
  const fits = (m: SavedMapping) => m.columns.length === columns && m.columns.every((c) => !c.startsWith('field:') || known.has(c));
  // The same header as last time: the user's own choices win over the guesses below.
  if (saved?.header && fits(saved) && saved.header.length === first.length && saved.header.every((h, i) => norm(h) === norm(first[i] ?? ''))) {
    return { columns: saved.columns.map((c) => once(decodeColumn(c))), header: true, origin: 'remembered' };
  }
  const fromHeader = first.map((cell) => headerTarget(cell, nt, attributes));
  // A header names most of its columns (one stray word in a data row is not a header).
  const filled = first.filter((c) => c.trim()).length;
  const named = fromHeader.filter((c) => c !== null).length;
  if (named > 0 && named * 2 >= filled) {
    const out = Array.from({ length: columns }, (_, i) => once(fromHeader[i] ?? null));
    return { columns: out, header: true, origin: 'header' };
  }
  if (saved && !saved.header && fits(saved)) {
    return { columns: saved.columns.map((c) => once(decodeColumn(c))), header: false, origin: 'remembered' };
  }
  const order = fillOrder(nt);
  const out = Array.from({ length: columns }, (_, i) => once(order[i] ? { kind: 'field', key: order[i]!.key } : null));
  return { columns: out, header: false, origin: 'order' };
}
