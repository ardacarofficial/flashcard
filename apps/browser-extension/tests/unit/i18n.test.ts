/** Rule 8: no fixed UI text — every string comes from `_locales` (TR + EN, same keys). */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { errorKeys } from '../../src/lib/errors';

const root = join(__dirname, '..', '..');
type Messages = Record<string, { message: string; placeholders?: Record<string, { content: string }> }>;
const load = (lang: string): Messages =>
  JSON.parse(readFileSync(join(root, 'public', '_locales', lang, 'messages.json'), 'utf8')) as Messages;
const en = load('en');
const tr = load('tr');

function files(dir: string, ext: RegExp): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p, ext) : ext.test(f) ? [p] : [];
  });
}

const sources = [
  ...files(join(root, 'src'), /\.ts$/),
  join(root, 'popup.html'),
  join(root, 'options.html'),
  join(root, 'public', 'manifest.json'),
];

/** Keys referenced as `t('key')`, `data-i18n*="key"`, `__MSG_key__`, or quoted message-like keys in ternaries. */
function usedKeys(): Set<string> {
  const keys = new Set<string>();
  const patterns = [
    /\bt\('([a-z0-9_]+)'/g,
    /data-i18n(?:-placeholder|-title|-aria)?="([a-z0-9_]+)"/g,
    /__MSG_([a-z0-9_]+)__/g,
    // (message-type names like `type: 'pair_poll'` / `case 'pair_poll'` are not text)
    /(?<!type: |case )'((?:conn|pair|queued|queue|options|class|error|added|front|offline|menu)_[a-z0-9_]+)'/g,
  ];
  for (const file of sources) {
    const text = readFileSync(file, 'utf8');
    for (const re of patterns) for (const m of text.matchAll(re)) if (m[1]) keys.add(m[1]);
  }
  return keys;
}

describe('locales', () => {
  it('TR and EN have the same keys and placeholders, none empty', () => {
    expect(Object.keys(tr).sort()).toEqual(Object.keys(en).sort());
    for (const [k, v] of Object.entries(en)) {
      const other = tr[k];
      expect(other, k).toBeDefined();
      if (!other) continue;
      expect(v.message.trim(), k).not.toBe('');
      expect(other.message.trim(), k).not.toBe('');
      expect(Object.keys(other.placeholders ?? {}).sort(), k).toEqual(Object.keys(v.placeholders ?? {}).sort());
      for (const name of Object.keys(v.placeholders ?? {})) {
        expect(v.message, k).toContain(`$${name}$`);
        expect(other.message, k).toContain(`$${name}$`);
      }
    }
  });

  it('every key used in code, pages and the manifest exists', () => {
    const used = usedKeys();
    expect(used.size).toBeGreaterThan(40);
    expect([...used].filter((k) => !(k in en))).toEqual([]);
  });

  it('every error category has a message', () => {
    for (const code of ['VALIDATION_X', 'NOT_FOUND_X', 'CONFLICT_X', 'PERMISSION_X', 'INTERNAL']) {
      const key = errorKeys(code)[1] ?? '';
      expect(key in en, code).toBe(true);
    }
  });

  it('pages contain no fixed text: every text node is empty or a symbol (labels come from data-i18n)', () => {
    for (const page of ['popup.html', 'options.html']) {
      const doc = new DOMParser().parseFromString(readFileSync(join(root, page), 'utf8'), 'text/html');
      const walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        const text = n.textContent?.trim() ?? '';
        expect(/\p{L}/u.test(text), `${page}: "${text}"`).toBe(false);
      }
    }
  });
});
