/**
 * docs/11 §4 / v1.2 brief: every colour pair the design system draws is WCAG AA in light and
 * dark. Reads `src/lib/theme/tokens.css` (the default preset) and checks the key pairs; the
 * backend's presets and custom accents are checked in fc_core::theme, and the frontend's
 * derived "ink" tokens for them (theme.svelte.ts) below.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import themesMock from '../src/lib/features/themes/mock';
import { createMockEnv } from '../src/lib/api/mock/env';
import { applyTokens } from '../src/lib/theme/theme.svelte';

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
  'surface-3': 'fc-surface-3',
  'text-1': 'fc-text',
  'text-2': 'fc-text-muted',
  accent: 'fc-accent',
  'accent-fg': 'fc-accent-text',
  'accent-text': 'fc-accent-ink',
  'focus-ring': 'fc-focus',
  danger: 'fc-danger',
  success: 'fc-success-ink',
};

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x! + 0.05) / (y! + 0.05);
}

/** Hex of an sRGB `color-mix(in srgb, a p%, b)`. */
function mix(a: string, b: string, p: number): string {
  const ch = (h: string, i: number) => parseInt(h.slice(i, i + 2), 16);
  return '#' + [1, 3, 5].map((i) => Math.round(ch(a, i) * p + ch(b, i) * (1 - p)).toString(16).padStart(2, '0')).join('');
}

/**
 * `--name: color-mix(in srgb, var(--a) N%, var(--b) | #hex | transparent)` tokens, resolved
 * per scheme; a tint over `transparent` is resolved over `--surface-1` (where it is drawn).
 */
function mixes(base: Record<Scheme, Record<string, string>>): Record<Scheme, Record<string, string>> {
  const root = css.slice(css.indexOf(':root {'), css.indexOf("\n}\n", css.indexOf(':root {')));
  const out: Record<Scheme, Record<string, string>> = { light: { ...base.light }, dark: { ...base.dark } };
  const ref = (s: Scheme, r: string) =>
    r.startsWith('#') ? r.toLowerCase() : r === 'transparent' ? out[s]['fc-surface'] : out[s][r.replace(/^var\(--|\)$/g, '')];
  for (const m of root.matchAll(/--([\w-]+):\s*color-mix\(in srgb, (var\(--[\w-]+\)|#[0-9a-f]{6}) (\d+)%, (var\(--[\w-]+\)|#[0-9a-f]{6}|transparent)\);/gi)) {
    for (const s of ['light', 'dark'] as const) {
      const [a, b] = [ref(s, m[2]!), ref(s, m[4]!)];
      if (a && b) out[s][m[1]!] = mix(a, b, Number(m[3]) / 100);
    }
  }
  return out;
}

const all = mixes(tokens());
const get = (s: Scheme, name: string): string => {
  const v = all[s][ALIAS[name] ?? name];
  if (!v) throw new Error(`no token --${name}`);
  return v;
};

const SURFACES = ['surface-0', 'surface-1', 'surface-2', 'surface-3'];
const STATUS_SURFACES = ['surface-0', 'surface-1', 'surface-2'];
/** [foreground, backgrounds, minimum ratio] */
const PAIRS: [string, string[], number][] = [
  ['text-1', SURFACES, 7],
  ['text-2', SURFACES, 4.5],
  ['text-1', ['accent-soft'], 4.5],
  // The accent as text sits on panels and inputs; on the selected step it marks icons (3:1).
  ['accent-text', ['surface-0', 'surface-1', 'surface-2', 'accent-soft'], 4.5],
  ['accent-text', ['surface-3'], 3],
  ['accent-fg', ['accent'], 4.5],
  ['accent-fg', ['accent-hover'], 4.5],
  ['focus-ring', SURFACES, 3],
  // Status text on panels and inputs (the selected step carries icons and chips, not status text).
  ['success', STATUS_SURFACES, 4.5],
  ['warning', STATUS_SURFACES, 4.5],
  ['danger', STATUS_SURFACES, 4.5],
  // Status text on its own soft tint (chips, badges, notices).
  ['success', ['success-soft'], 4.5],
  ['warning', ['warning-soft'], 4.5],
  ['danger', ['danger-soft'], 4.5],
  ['success-solid-fg', ['success-solid'], 4.5],
  ['warning-solid-fg', ['warning-solid'], 4.5],
  ['danger-solid-fg', ['danger-solid'], 4.5],
];

describe('design tokens contrast (WCAG AA)', () => {
  it('the default accent is amber #F5B544 (v1.2), darker as text in light', () => {
    expect(get('light', 'accent')).toBe('#f5b544');
    expect(get('dark', 'accent')).toBe('#f5b544');
    expect(get('light', 'accent-text')).toBe('#9a5b00');
    expect(get('dark', 'surface-0')).toBe('#0e0e10');
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

  it('a custom accent as text (derived `--fc-accent-ink`) is AA on every surface', () => {
    const failures: string[] = [];
    const text = { light: get('light', 'text-1'), dark: get('dark', 'text-1') };
    const bg = { light: get('light', 'surface-0'), dark: get('dark', 'surface-0') };
    for (let r = 0; r < 256; r += 17) {
      for (let g = 0; g < 256; g += 17) {
        for (let b = 0; b < 256; b += 17) {
          const hex = '#' + [r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('');
          const ok = { light: contrast(hex, bg.light) >= 3, dark: contrast(hex, bg.dark) >= 3 };
          const root = document.createElement('div');
          // The backend only promises 3:1 against the background for an accent.
          applyTokens({ preset: 'default', accent: hex, light: { accent: hex, text: text.light }, dark: { accent: hex, text: text.dark } }, root);
          const value = root.style.getPropertyValue('--fc-accent-ink');
          const parts = [...value.matchAll(/color-mix\(in srgb, (#[0-9a-f]{6}) (\d+)%, (#[0-9a-f]{6})\)/g)];
          expect(parts).toHaveLength(2);
          for (const [i, scheme] of (['light', 'dark'] as const).entries()) {
            if (!ok[scheme]) continue;
            const m = parts[i]!;
            const ink = mix(m[1]!, m[3]!, Number(m[2]) / 100);
            for (const surface of ['surface-0', 'surface-1', 'surface-2']) {
              const c = contrast(ink, get(scheme, surface));
              if (c < 4.5) failures.push(`${scheme} ${hex} → ${ink} on ${surface}: ${c.toFixed(2)}`);
            }
          }
        }
      }
    }
    expect(failures).toEqual([]);
  });
});
