/**
 * v1.1 polish: numbers, percentages, durations and dates are formatted only by `$lib/format`
 * (Intl, active locale: TR `%35`, `15,0 sn`; EN `35%`, `15.0 s`). Components never call
 * `toFixed(` or glue a `%` to a number themselves. CSS lengths (`style:width="{x}%"`,
 * `calc(… * 100%)`) are not display text and pass.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { describe, expect, it } from 'vitest';

const APP = join(__dirname, '..');
const SRC = join(APP, 'src');

function svelteFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return svelteFiles(p);
    return name.endsWith('.svelte') ? [p] : [];
  });
}

/** Ad-hoc number formatting in one component's source (line numbers, 1-based). */
export function adHocFormatting(source: string): string[] {
  // CSS is not display text.
  const code = source.replace(/<style[\s\S]*?<\/style>/g, (m) => m.replace(/[^\n]/g, ' '));
  const out: string[] = [];
  const lines = code.split('\n');
  lines.forEach((line, i) => {
    // `style:x="{…}%"` / `style="width: {…}%"` set CSS lengths.
    const text = line.replace(/style(:[\w-]+)?=("[^"]*"|\{[^}]*\})/g, '');
    if (/\.toFixed\(/.test(text)) out.push(`${i + 1}: toFixed(`);
    // `{n}%`, `${n}%`, `n + '%'`, `'%' + n` in markup or script.
    if (/\}%/.test(text) || /\+\s*['"`]%|['"`]%['"`]?\s*\+/.test(text)) out.push(`${i + 1}: manual %`);
  });
  return out;
}

const rel = (p: string) => relative(APP, p).split(sep).join('/');

describe('number formatting goes through $lib/format', () => {
  it('flags toFixed and hand-made percentages, not CSS lengths', () => {
    expect(adHocFormatting('<span>{n.toFixed(1)}</span>')).toEqual(['1: toFixed(']);
    expect(adHocFormatting('<span>{pct}%</span>')).toEqual(['1: manual %']);
    expect(adHocFormatting("<script>const s = `${n}%`;</script>")).toEqual(['1: manual %']);
    expect(adHocFormatting("<script>const s = n + '%';</script>")).toEqual(['1: manual %']);
    expect(adHocFormatting('<div style:width="{p * 100}%"></div>')).toEqual([]);
    expect(adHocFormatting('<div style="width: {p}%"></div>')).toEqual([]);
    expect(adHocFormatting('<style>.a { width: 100%; }</style>')).toEqual([]);
    expect(adHocFormatting('<script>const css = (v) => `calc(${v} * 100%)`;</script>')).toEqual([]);
  });

  it('no component formats numbers by hand', () => {
    const found = svelteFiles(SRC).flatMap((f) => adHocFormatting(readFileSync(f, 'utf8')).map((m) => `${rel(f)}:${m}`));
    expect(found, 'use formatPercent / formatDecimal / formatSeconds … from $lib/format').toEqual([]);
  });
});
