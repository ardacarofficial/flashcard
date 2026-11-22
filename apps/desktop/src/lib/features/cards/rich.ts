/**
 * Typesetting of rendered card HTML in the DOM: KaTeX for `span.math`, highlight.js for
 * `code.language-*`, labels for hint toggles and for the labelled answer parts of the
 * built-in vocabulary backs (v1.3: `span.fc-part-label[data-fc-part]` → `cards.part.<part>`).
 * Runs on HTML the backend already sanitized; KaTeX runs with `trust: false`.
 */
import { td } from '$lib/i18n/index.svelte';

/** Answer parts that have a label (`cards.part.*`). */
const PART_LABELS = new Set(['example', 'source']);
// Loaded on first use so cards without math/code don't pay for them.
const loadKatex = () => import('katex').then((m) => m.default);
const loadHljs = () => import('highlight.js/lib/common').then((m) => m.default);

export interface EnhanceOptions {
  math: boolean;
  code: boolean;
  hintLabel: string;
}

export async function enhance(root: HTMLElement, opts: EnhanceOptions): Promise<void> {
  for (const s of root.querySelectorAll<HTMLElement>('details.fc-hint > summary')) {
    if (!s.textContent) s.textContent = opts.hintLabel;
  }
  for (const l of root.querySelectorAll<HTMLElement>('.fc-part-label[data-fc-part]')) {
    const part = l.dataset['fcPart'] ?? '';
    if (!l.textContent && PART_LABELS.has(part)) l.textContent = td(`cards.part.${part}`);
  }
  const maths = opts.math ? [...root.querySelectorAll<HTMLElement>('span.math')] : [];
  if (maths.length) {
    const katex = await loadKatex();
    for (const el of maths) {
      if (el.dataset['typeset']) continue;
      const tex = el.textContent ?? '';
      try {
        katex.render(tex, el, {
          displayMode: el.classList.contains('math-display'),
          throwOnError: false,
          trust: false,
        });
        el.dataset['typeset'] = '1';
      } catch {
        el.textContent = tex;
      }
    }
  }
  const codes = opts.code ? [...root.querySelectorAll<HTMLElement>('pre code[class*="language-"]')] : [];
  if (codes.length) {
    const hljs = await loadHljs();
    for (const el of codes) {
      if (!el.dataset['highlighted']) hljs.highlightElement(el);
    }
  }
}
