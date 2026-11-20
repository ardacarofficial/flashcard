/**
 * docs/11 §4: every colour pair the design system draws is WCAG AA in light and dark.
 * Reads `src/lib/theme/tokens.css` (the default preset) and checks the key pairs; the
 * backend's presets and custom accents are checked in fc_core::theme.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import themesMock from '../src/lib/features/themes/mock';
import { createMockEnv } from '../src/lib/api/mock/env';

const css = readFileSync(join(__dirname, '..', 'src', 'lib', 'theme', 'tokens.css'), 'utf8');

type Scheme = 'light' | 'dark';

/** `--name: light-dark(#a, #b)` or `--name: #a` → per-scheme hex. */
function tokens(): Record<Scheme, Record<string, string>> {
  const out: Record<Scheme, Record<string, string>> = { light: {}, dark: {} };
  const root = css.slice(css.indexOf(':root {'), css.indexOf("\n}\n", css.indexOf(':root {')));
  for (const m of root.matchAll(/--([\w-]+):\s*(light-dark\((#[0-9a-f]{6}),\s*(#[0-9a-f]{6})\)|(#[0-9a-f]{6}));/gi)) {
    out.light[m[1]!] = (m[3] ?? m[5])!.toLowerCase();
    out.dark[m[1]!] = (m[4] ?? m[5])!.toLowerCase();
  }
  return out;
}

/** Aliases in the CSS (`--surface-0: var(--fc-bg)`). */
const ALIAS: Record<string, string> = {
  'surface-0': 'fc-bg',
  'surface-1': 'fc-surface',
  'surface-2': 'fc-bg-subtle',
  'text-1': 'fc-text',
  'text-2': 'fc-text-muted',
  accent: 'fc-accent',
  'accent-fg': 'fc-accent-text',
  'accent-soft': 'fc-selected',
  'focus-ring': 'fc-focus',
  success: 'fc-success',
  danger: 'fc-danger',
};

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x! + 0.05) / (y! + 0.05);
}

const all = tokens();
const get = (s: Scheme, name: string): string => {
  const v = all[s][ALIAS[name] ?? name];
  if (!v) throw new Error(`no token --${name}`);
  return v;
};

const SURFACES = ['surface-0', 'surface-1', 'surface-2'];
/** [foreground, backgrounds, minimum ratio] */
const PAIRS: [string, string[], number][] = [
  ['text-1', SURFACES, 7],
  ['text-2', SURFACES, 4.5],
  ['text-1', ['accent-soft'], 4.5],
  ['accent', SURFACES, 4.5],
  ['accent-fg', ['accent'], 4.5],
  ['focus-ring', SURFACES, 3],
  ['success', SURFACES, 4.5],
  ['warning', SURFACES, 4.5],
  ['danger', SURFACES, 4.5],
  ['success-solid-fg', ['success-solid'], 4.5],
  ['warning-solid-fg', ['warning-solid'], 4.5],
  ['danger-solid-fg', ['danger-solid'], 4.5],
];

describe('design tokens contrast (WCAG AA)', () => {
  it('the default accent is #5B47E0', () => {
    expect(get('light', 'accent')).toBe('#5b47e0');
  });

  for (const scheme of ['light', 'dark'] as const) {
    it(`${scheme}: key pairs pass`, () => {
      const failures: string[] = [];
      for (const [fg, bgs, min] of PAIRS) {
        for (const bg of bgs) {
          const c = contrast(get(scheme, fg), get(scheme, bg));
          if (c < min) failures.push(`${fg} on ${bg}: ${c.toFixed(2)} < ${min}`);
        }
      }
      expect(failures).toEqual([]);
    });
  }

  it('the mock backend paints the same default preset as tokens.css (and fc_core)', async () => {
    const handlers = themesMock.handlers(createMockEnv());
    const theme = (handlers['themes.current'] as () => import('../src/lib/api/types').ThemeDto)();
    for (const scheme of ['light', 'dark'] as const) {
      for (const [name, value] of Object.entries(theme[scheme])) expect([scheme, name, value]).toEqual([scheme, name, all[scheme][`fc-${name}`]]);
    }
  });
});
