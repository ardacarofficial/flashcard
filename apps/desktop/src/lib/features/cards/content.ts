/**
 * Display-side helpers for card content. Pure string/geometry functions only: everything
 * that decides *what* a card shows (templates, sanitizing, link resolution) is in Rust.
 */
import type { FieldDef, MediaDto, NoteTypeDto, NoteTypeInput } from '$lib/api/types';

const MEDIA_SRC = /src="media:([0-9a-f]{64})"/g;

/** Replaces backend media references (`src="media:<sha256>"`) with displayable URLs. */
export function mapMediaSrc(html: string, urlOf: (hash: string) => string): string {
  return html.replace(MEDIA_SRC, (_m, hash: string) => `src="${urlOf(hash)}"`);
}

/** Media hashes referenced in rendered HTML, in order, without duplicates. */
export function mediaHashes(html: string): string[] {
  return [...new Set([...html.matchAll(MEDIA_SRC)].map((m) => m[1]!))];
}

/** First `media:<sha256>` in a field value (image fields hold one or more references). */
export function firstMediaHash(value: string): string | null {
  return /media:([0-9a-f]{64})/.exec(value)?.[1] ?? null;
}

export interface Edit {
  text: string;
  /** Selection start/end after the edit. */
  start: number;
  end: number;
}

/** Wraps the selection in `before`/`after` (or inserts them around the cursor). */
export function wrapSelection(text: string, start: number, end: number, before: string, after: string): Edit {
  const selected = text.slice(start, end);
  return {
    text: text.slice(0, start) + before + selected + after + text.slice(end),
    start: start + before.length,
    end: start + before.length + selected.length,
  };
}

/** Replaces the selection with `insert`, cursor after it. */
export function insertAt(text: string, start: number, end: number, insert: string): Edit {
  const pos = start + insert.length;
  return { text: text.slice(0, start) + insert + text.slice(end), start: pos, end: pos };
}

/** The text typed after an unclosed `[[` right before the cursor (for link completion). */
export function linkQueryAt(text: string, cursor: number): string | null {
  const before = text.slice(0, cursor);
  const open = before.lastIndexOf('[[');
  if (open < 0) return null;
  const query = before.slice(open + 2);
  if (query.includes(']]') || query.includes('\n') || query.includes('[')) return null;
  return query;
}

/** Completes the `[[query` before the cursor to `[[title]]`. */
export function completeLink(text: string, cursor: number, title: string): Edit {
  const open = text.slice(0, cursor).lastIndexOf('[[');
  const closeAhead = text.slice(cursor).startsWith(']]') ? 2 : 0;
  const insert = `[[${title}]]`;
  const next = text.slice(0, open) + insert + text.slice(cursor + closeAhead);
  const pos = open + insert.length;
  return { text: next, start: pos, end: pos };
}

/** What to put into a Markdown field for an uploaded file. */
export function mediaSnippet(m: Pick<MediaDto, 'kind' | 'reference' | 'original_name'>): string {
  const alt = (m.original_name ?? '').replace(/[[\]()]/g, '');
  switch (m.kind) {
    case 'image':
      return `![${alt}](${m.reference})`;
    case 'audio':
      return `<audio controls src="${m.reference}"></audio>`;
    case 'video':
      return `<video controls src="${m.reference}"></video>`;
  }
}

/**
 * Whether `css` has balanced `{}`/`()`/`[]` outside strings and comments and no `<`, i.e.
 * cannot close a block it did not open or end the `<style>` element.
 */
export function cssIsContained(css: string): boolean {
  const stack: string[] = [];
  const close: Partial<Record<string, string>> = { '{': '}', '(': ')', '[': ']' };
  for (let i = 0; i < css.length; i++) {
    const c = css.charAt(i);
    if (c === '<') return false;
    if (c === '\\') {
      i++;
      continue;
    }
    if (c === '/' && css[i + 1] === '*') {
      const end = css.indexOf('*/', i + 2);
      if (end < 0) return false;
      i = end + 1;
      continue;
    }
    if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < css.length && css[j] !== c) {
        if (css[j] === '\n' || css[j] === '\r' || css[j] === '\f') return false;
        if (css[j] === '\\') j++;
        j++;
      }
      if (j >= css.length) return false;
      i = j;
      continue;
    }
    const want = close[c];
    if (want) stack.push(want);
    else if (c === '}' || c === ')' || c === ']') {
      if (stack.pop() !== c) return false;
    }
  }
  return stack.length === 0;
}

/**
 * Note type CSS scoped to the card container (CSS nesting). The backend already sanitizes and
 * balances it (`fc_core::content::sanitize_css`); anything that could still leave the scope
 * is dropped as a second line of defence.
 */
export function scopeCss(css: string, scope: string): string {
  if (!css.trim() || !cssIsContained(css)) return '';
  return `.card.${scope} {\n${css}\n}`;
}

/** Editable copy of a note type. */
export function toInput(nt: NoteTypeDto): NoteTypeInput {
  return {
    name: nt.name,
    kind: nt.kind,
    fields: nt.fields.map((f) => ({ ...f })),
    templates: nt.templates.map((t) => ({ ...t })),
    css: nt.css,
    sort_field: nt.sort_field,
  };
}

/** A new, empty field (the backend assigns its key from the name). */
export function newField(name: string): FieldDef {
  return {
    key: '',
    name,
    kind: 'rich_text',
    required: false,
    unique: false,
    searchable: true,
    default_value: null,
  };
}

/** Moves item `i` by `delta` (−1 up, +1 down); out-of-range moves are ignored. */
export function move<T>(items: T[], i: number, delta: number): T[] {
  const j = i + delta;
  if (j < 0 || j >= items.length) return items;
  const out = [...items];
  [out[i], out[j]] = [out[j]!, out[i]!];
  return out;
}

/** Sample values for previewing a note type definition: each field shows its own name. */
export function sampleFields(def: NoteTypeInput): Record<string, string> {
  const out: Record<string, string> = {};
  def.fields.forEach((f, i) => {
    if (!f.key || f.kind === 'image' || f.kind === 'audio' || f.kind === 'video') return;
    out[f.key] = def.kind === 'cloze' && i === 0 ? `{{c1::${f.name}}} …` : f.name;
  });
  return out;
}

// ------------------------------------------------------------------ image occlusion masks

export interface Mask {
  n: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Masks {
  mode: 'hide_all' | 'hide_one';
  masks: Mask[];
}

/** Parses the `masks` field (lenient: bad JSON = no masks). */
export function parseMasks(raw: string): Masks {
  try {
    const v = JSON.parse(raw) as Partial<Masks>;
    return {
      mode: v.mode === 'hide_one' ? 'hide_one' : 'hide_all',
      masks: Array.isArray(v.masks) ? v.masks : [],
    };
  } catch {
    return { mode: 'hide_all', masks: [] };
  }
}

export function serializeMasks(m: Masks): string {
  return m.masks.length ? JSON.stringify(m) : '';
}

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const round = (v: number) => Math.round(v * 10000) / 10000;

/** A mask from two corner points (fractions); numbered after the highest existing one. */
export function maskFromDrag(existing: Mask[], x0: number, y0: number, x1: number, y1: number): Mask | null {
  const [ax, bx] = [clamp01(Math.min(x0, x1)), clamp01(Math.max(x0, x1))];
  const [ay, by] = [clamp01(Math.min(y0, y1)), clamp01(Math.max(y0, y1))];
  if (bx - ax < 0.01 || by - ay < 0.01) return null;
  const n = existing.reduce((max, m) => Math.max(max, m.n || 0), 0) + 1;
  return { n, x: round(ax), y: round(ay), w: round(bx - ax), h: round(by - ay) };
}
