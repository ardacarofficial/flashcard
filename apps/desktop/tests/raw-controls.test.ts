/**
 * docs/11 §4/§6: "Hiçbir ekranda ham tarayıcı `<select>`/checkbox yok (lint testi)". Outside
 * `src/lib/ui/` no component may use a raw `<select>`, `<input type="checkbox">` or
 * `<input type="radio">`; use `Select`, `Checkbox`/`Switch`, `RadioGroup`/`SegmentedControl`
 * from `$lib/ui`.
 *
 * v1.0 screens still have some: they are listed, with their count, in
 * `tests/raw-controls.allow.json`. The list only shrinks — a screen lane that restyles a
 * screen removes its entry (the test fails while an entry is larger than needed, so a fixed
 * file cannot keep its allowance). New files are never added to it.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { parse } from 'svelte/compiler';
import { describe, expect, it } from 'vitest';
import allowList from './raw-controls.allow.json';

const APP = join(__dirname, '..');
const SRC = join(APP, 'src');
const UI = join(SRC, 'lib', 'ui');

function svelteFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return p === UI ? [] : svelteFiles(p);
    return name.endsWith('.svelte') ? [p] : [];
  });
}

interface Node {
  type: string;
  [k: string]: unknown;
}

/** Static value of an attribute (`type="checkbox"`), or null when dynamic/absent. */
function staticAttr(el: Node, name: string): string | null {
  const attr = (el.attributes as Node[]).find((a) => a.type === 'Attribute' && a.name === name);
  if (!attr || !Array.isArray(attr.value)) return null;
  const parts = attr.value as Node[];
  return parts.every((p) => p.type === 'Text') ? parts.map((p) => p.data as string).join('') : null;
}

/** Raw controls in one component: `select`, `input[type=checkbox|radio]`. */
export function rawControls(source: string): string[] {
  const ast = parse(source, { modern: true }) as unknown as { fragment: Node };
  const out: string[] = [];
  const walk = (v: unknown): void => {
    if (Array.isArray(v)) return v.forEach(walk);
    if (typeof v !== 'object' || v === null) return;
    const n = v as Node;
    if (n.type === 'RegularElement') {
      if (n.name === 'select') out.push('select');
      if (n.name === 'input') {
        const type = staticAttr(n, 'type');
        if (type === 'checkbox' || type === 'radio') out.push(`input[type=${type}]`);
      }
    }
    for (const [k, child] of Object.entries(n)) if (k !== 'parent') walk(child);
  };
  walk(ast.fragment);
  return out;
}

const rel = (p: string) => relative(APP, p).split(sep).join('/');

describe('no raw form controls outside $lib/ui', () => {
  it('detects select / checkbox / radio, not other inputs', () => {
    expect(
      rawControls('<select><option>1</option></select><input type="checkbox" /><input type="radio" /><input type="text" /><input />'),
    ).toEqual(['select', 'input[type=checkbox]', 'input[type=radio]']);
  });

  const found = new Map(svelteFiles(SRC).map((f) => [rel(f), rawControls(readFileSync(f, 'utf8')).length]));
  const allowed = allowList as Record<string, number>;

  it('only allow-listed v1.0 screens still have them (and no more than listed)', () => {
    const over = [...found].filter(([f, n]) => n > (allowed[f] ?? 0)).map(([f, n]) => `${f}: ${n} raw control(s), allowed ${allowed[f] ?? 0}`);
    expect(over, 'use Select / Checkbox / Switch / RadioGroup / SegmentedControl from $lib/ui').toEqual([]);
  });

  it('the allow-list only shrinks: entries match what is left', () => {
    const stale = Object.entries(allowed)
      .filter(([f, n]) => (found.get(f) ?? 0) !== n)
      .map(([f, n]) => `${f}: listed ${n}, found ${found.get(f) ?? 0} — lower or remove the entry in tests/raw-controls.allow.json`);
    expect(stale).toEqual([]);
  });
});
