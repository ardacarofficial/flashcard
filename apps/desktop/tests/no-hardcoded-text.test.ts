/**
 * Rule 8 (CLAUDE.md): no hard-coded UI text. Every visible string in a
 * component must come from an i18n key. This scans markup (not <script>/<style>)
 * for literal text nodes and literal user-facing attributes containing letters.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = join(__dirname, '..', 'src');

function svelteFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return svelteFiles(p);
    return p.endsWith('.svelte') ? [p] : [];
  });
}

const LETTER = /\p{L}/u;

function stripExpressions(s: string): string {
  // Remove {…} blocks innermost-first so nested braces are handled.
  let prev = '';
  while (prev !== s) {
    prev = s;
    s = s.replace(/\{[^{}]*\}/g, '');
  }
  return s;
}

export function findRawText(source: string): string[] {
  const markup = stripExpressions(source
    .replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, ''),
  );
  const hits: string[] = [];
  for (const m of markup.matchAll(/>([^<>]*)</g)) {
    const text = m[1]!.trim();
    if (text && LETTER.test(text)) hits.push(text);
  }
  for (const m of markup.matchAll(/\b(placeholder|title|aria-label|alt|label)="([^"]*)"/g)) {
    if (LETTER.test(m[2]!)) hits.push(`${m[1]}="${m[2]}"`);
  }
  return hits;
}

describe('no hard-coded UI text', () => {
  it('detector catches literals', () => {
    expect(findRawText('<p>Hello</p><b>{t("x", { n: 1 })}</b><i title="Hi"></i>')).toEqual(['Hello', 'title="Hi"']);
  });

  for (const file of svelteFiles(SRC)) {
    it(relative(SRC, file), () => {
      expect(findRawText(readFileSync(file, 'utf8'))).toEqual([]);
    });
  }
});
