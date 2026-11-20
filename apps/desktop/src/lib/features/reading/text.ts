import type { TextAnalysis, WordStatus, WordToken } from '$lib/api/types';

/** A run of the text: a word (with its token) or the text between words. */
export interface Segment {
  start: number;
  end: number;
  text: string;
  token: WordToken | null;
}

/**
 * Cuts `text` into words and gaps for display. Offsets are UTF-16 (JS string indices), as the
 * backend sends them; nothing is computed here beyond slicing.
 */
export function segments(text: string, tokens: WordToken[], from = 0, to = text.length): Segment[] {
  const out: Segment[] = [];
  let pos = from;
  for (const tok of tokens) {
    if (tok.end <= from || tok.start >= to) continue;
    if (tok.start > pos) out.push({ start: pos, end: tok.start, text: text.slice(pos, tok.start), token: null });
    out.push({ start: tok.start, end: tok.end, text: text.slice(tok.start, tok.end), token: tok });
    pos = tok.end;
  }
  if (pos < to) out.push({ start: pos, end: to, text: text.slice(pos, to), token: null });
  return out;
}

/** The sentence range around `offset` (from the backend's sentence list). */
export function sentenceAround(a: TextAnalysis, offset: number): [number, number] | null {
  return (a.sentences.find(([s, e]) => offset >= s && offset < e) as [number, number] | undefined) ?? null;
}

/** Lines of a subtitle text with their start offsets (line `i` is cue `i`). */
export function lines(text: string): { start: number; end: number }[] {
  const out: { start: number; end: number }[] = [];
  let start = 0;
  for (const line of text.split('\n')) {
    out.push({ start, end: start + line.length });
    start += line.length + 1;
  }
  return out;
}

/** Text offset of a DOM position inside an element whose segments carry `data-start`. */
export function domOffset(node: Node | null, offset: number): number | null {
  let el: Node | null = node;
  while (el && !(el instanceof HTMLElement && el.dataset.start !== undefined)) el = el.parentNode;
  if (!(el instanceof HTMLElement)) return null;
  const base = Number(el.dataset.start);
  return node instanceof Text ? base + offset : base;
}

/** The selected text range (UTF-16) inside the reader, if any. */
export function selectedRange(): [number, number] | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const r = sel.getRangeAt(0);
  const a = domOffset(r.startContainer, r.startOffset);
  const b = domOffset(r.endContainer, r.endOffset);
  if (a === null || b === null || a === b) return null;
  return a < b ? [a, b] : [b, a];
}

/** `m:ss` of a cue start. */
export function clock(ms: number | bigint): string {
  const s = Math.floor(Number(ms) / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** Display order of the statuses. */
export const STATUSES: WordStatus[] = ['new', 'learning', 'known', 'ignored'];
