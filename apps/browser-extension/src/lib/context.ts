/** Pure helpers that turn a raw page selection into what goes to the inbox. */

/** What the in-page script reports (see `capturePage`). */
export type PageSelection = {
  /** Selected text. */
  text: string;
  /** Text of the nearest block element around the selection ('' when unknown). */
  block: string;
  /** Selection start/end offsets within `block` (-1 when unknown). */
  start: number;
  end: number;
  /** Page title and address. */
  title: string;
  url: string;
};

/** A selection ready for the popup. */
export type Capture = {
  /** The word/phrase: becomes the card front. */
  front: string;
  /** The sentence it appeared in ('' if it could not be found or is the selection itself). */
  sentence: string;
  title: string;
  url: string;
};

/** A capture waiting for the popup, stamped so a stale one is not offered later. */
export type StoredCapture = Capture & { capturedAt: number };

/** A capture older than this is not offered to the popup any more. */
export const CAPTURE_TTL_MS = 10 * 60 * 1000;

/** Longest sentence kept as context; longer ones are cut around the selection. */
export const MAX_SENTENCE = 400;
/** Longest selection accepted as a card front. */
export const MAX_FRONT = 1000;

const BOUNDARY = /[.!?…。！？]/u;

/** Collapses runs of whitespace (incl. newlines/nbsp) into single spaces and trims. */
export function squash(text: string): string {
  return text.replace(/\s+/gu, ' ').trim();
}

/**
 * The sentence of `block` that contains `[start, end)`. Boundaries are `. ! ? …` (and CJK
 * equivalents) followed by whitespace or the end, and line breaks. Very long sentences are cut to a
 * window around the selection, marked with `…`.
 */
export function sentenceAround(block: string, start: number, end: number): string {
  if (block === '' || start < 0 || end < start || end > block.length) return '';
  let from = 0;
  for (let i = start - 1; i >= 0; i--) {
    const ch = block[i] ?? '';
    if (ch === '\n') {
      from = i + 1;
      break;
    }
    if (BOUNDARY.test(ch) && /\s/u.test(block[i + 1] ?? ' ')) {
      from = i + 1;
      break;
    }
  }
  let to = block.length;
  for (let i = Math.max(end, start); i < block.length; i++) {
    const ch = block[i] ?? '';
    if (ch === '\n') {
      to = i;
      break;
    }
    if (BOUNDARY.test(ch) && /\s/u.test(block[i + 1] ?? ' ')) {
      // keep consecutive closing punctuation (`?!`, `...`)
      let j = i + 1;
      while (j < block.length && BOUNDARY.test(block[j] ?? '')) j++;
      to = j;
      break;
    }
  }
  let sentence = block.slice(from, to);
  let selStart = start - from;
  if (sentence.length > MAX_SENTENCE) {
    const half = Math.floor((MAX_SENTENCE - (end - start)) / 2);
    const a = Math.max(0, selStart - Math.max(half, 0));
    const b = Math.min(sentence.length, a + MAX_SENTENCE);
    sentence = (a > 0 ? '…' : '') + sentence.slice(a, b) + (b < sentence.length ? '…' : '');
    selStart = 0;
  }
  return squash(sentence);
}

/** Builds the capture from what the page reported, or from the context-menu fallback. */
export function toCapture(sel: PageSelection): Capture {
  const front = squash(sel.text).slice(0, MAX_FRONT);
  let sentence = sentenceAround(sel.block, sel.start, sel.end);
  if (squash(sentence) === front) sentence = '';
  return { front, sentence, title: squash(sel.title), url: sel.url };
}

/**
 * Runs **inside the page** (`chrome.scripting.executeScript`), so it must stay self-contained:
 * no imports, no closures. Reports the selection plus the text of its nearest block element.
 */
export function capturePage(): PageSelection {
  const sel = window.getSelection();
  const text = sel ? sel.toString() : '';
  const base = { text, block: '', start: -1, end: -1, title: document.title, url: location.href };
  if (!sel || sel.rangeCount === 0 || text.trim() === '') return base;
  const range = sel.getRangeAt(0);
  let node: Node | null = range.startContainer;
  const blockTags = /^(P|LI|TD|TH|DD|DT|BLOCKQUOTE|H[1-6]|PRE|ARTICLE|SECTION|DIV|BODY|FIGCAPTION|CAPTION)$/;
  while (node && !(node instanceof HTMLElement && blockTags.test(node.tagName))) {
    node = node.parentNode;
  }
  if (!(node instanceof HTMLElement)) return base;
  const before = document.createRange();
  before.setStart(node, 0);
  before.setEnd(range.startContainer, range.startOffset);
  const block = node.innerText || node.textContent || '';
  const prefix = before.toString();
  // innerText and Range text can differ in whitespace; find the selection near the expected spot.
  const guess = prefix.length;
  let start = block.indexOf(text, Math.max(0, guess - 200));
  if (start < 0) start = block.indexOf(text);
  if (start < 0) return base;
  // A page-sized block (DIV/BODY) is trimmed to a window around the selection.
  const from = Math.max(0, start - 2000);
  const to = Math.min(block.length, start + text.length + 2000);
  return { ...base, block: block.slice(from, to), start: start - from, end: start - from + text.length };
}
