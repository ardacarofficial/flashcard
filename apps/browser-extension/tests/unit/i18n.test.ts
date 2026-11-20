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

  it('pages have no fixed text in user-visible attributes (they come from data-i18n-*)', () => {
    const shown = ['title', 'placeholder', 'aria-label', 'aria-description', 'alt', 'label'];
    for (const page of ['popup.html', 'options.html']) {
      const doc = new DOMParser().parseFromString(readFileSync(join(root, page), 'utf8'), 'text/html');
      for (const el of doc.querySelectorAll('*')) {
        for (const name of shown) {
          const v = el.getAttribute(name) ?? '';
          expect(/\p{L}/u.test(v), `${page}: <${el.tagName.toLowerCase()} ${name}="${v}">`).toBe(false);
        }
        if (el instanceof HTMLInputElement && ['button', 'submit', 'reset'].includes(el.type)) {
          expect(/\p{L}/u.test(el.value), `${page}: button value "${el.value}"`).toBe(false);
        }
      }
    }
  });

  it('the manifest shows only __MSG_…__ texts', () => {
    const m = JSON.parse(readFileSync(join(root, 'public', 'manifest.json'), 'utf8')) as {
      name: string;
      short_name?: string;
      description: string;
      action?: { default_title?: string };
      commands?: Record<string, { description?: string }>;
    };
    const texts = [m.name, m.short_name, m.description, m.action?.default_title, ...Object.values(m.commands ?? {}).map((c) => c.description)];
    for (const text of texts) if (text !== undefined) expect(text, text).toMatch(/^__MSG_[a-z0-9_]+__$/);
  });

  it('scripts set no literal text on the page', () => {
    // Literal (letters) assigned to a shown property, set as a shown attribute, or passed to
    // confirm/alert/notifications — text must come from t('key').
    const sinks = [
      /\.(?:textContent|innerText|innerHTML|title|placeholder|ariaLabel|alt|label)\s*=\s*(['"`])((?:(?!\1)[^\\]|\\.)*)\1/g,
      /setAttribute\(\s*'(?:title|placeholder|aria-label|alt|label)'\s*,\s*(['"`])((?:(?!\1)[^\\]|\\.)*)\1/g,
      /\b(?:confirm|alert|prompt)\(\s*(['"`])((?:(?!\1)[^\\]|\\.)*)\1/g,
      /\btitle:\s*(['"`])((?:(?!\1)[^\\]|\\.)*)\1/g,
    ];
    const found: string[] = [];
    for (const file of files(join(root, 'src'), /\.ts$/)) {
      const text = readFileSync(file, 'utf8');
      for (const re of sinks) {
        for (const m of text.matchAll(re)) {
          const literal = (m[2] ?? '').replace(/\$\{[^}]*\}/g, '');
          if (/\p{L}/u.test(literal)) found.push(`${file.slice(root.length + 1)}: ${m[0]}`);
        }
      }
    }
    expect(found).toEqual([]);
  });

  it('the sink check catches literals', () => {
    const re = /\.(?:textContent|title)\s*=\s*(['"`])((?:(?!\1)[^\\]|\\.)*)\1/g;
    const hits = [...`a.textContent = 'Saved'; b.title = t('x'); c.textContent = \`\${n}\`;`.matchAll(re)]
      .map((m) => (m[2] ?? '').replace(/\$\{[^}]*\}/g, ''))
      .filter((l) => /\p{L}/u.test(l));
    expect(hits).toEqual(['Saved']);
  });
});
