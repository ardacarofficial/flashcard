/**
 * docs/11 §4/§6: "Hiçbir ekranda ham tarayıcı `<select>`/checkbox yok (lint testi)". Outside
 * `src/lib/ui/` no component may use a raw `<select>`, `<input type="checkbox">` or
 * `<input type="radio">`; use `Select`, `Checkbox`/`Switch`, `RadioGroup`/`SegmentedControl`
 * from `$lib/ui`.
 *
 * v1.1: no exceptions — the v1.0 allow-list is gone, every screen is on the design system.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { parse } from 'svelte/compiler';
import { describe, expect, it } from 'vitest';

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

  it('no component outside $lib/ui has one (no allow-list)', () => {
    const found = svelteFiles(SRC)
      .map((f) => [rel(f), rawControls(readFileSync(f, 'utf8'))] as const)
      .filter(([, raw]) => raw.length > 0)
      .map(([f, raw]) => `${f}: ${raw.join(', ')}`);
    expect(found, 'use Select / Checkbox / Switch / RadioGroup / SegmentedControl from $lib/ui').toEqual([]);
  });
});
