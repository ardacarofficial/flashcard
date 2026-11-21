/**
 * Dev-only mock of the v1.0 `themes` module: the five built-in presets and the custom-accent
 * rules of `fc_core::theme` (accent pushed to ≥ 3:1 on the background, black or white text on
 * the fill, `selected` mixed toward the background), so `pnpm dev:mock` shows what the app
 * really paints. `fc_core::theme` stays the source of truth; keep the values in step with it.
 */
import { module, settingsPage, type MockFeature } from '$lib/api/mock/env';
import type { ThemeDto, ThemeMode, ThemePresetDto } from '$lib/api/types';

type Rgb = [number, number, number];
type Scheme = 'light' | 'dark';
/** bg, bg-subtle, surface, text, text-muted, border, accent, focus (or ''), danger, success. */
type Base = [string, string, string, string, string, string, string, string, string, string];

const parse = (hex: string): Rgb => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb;
const hex = (c: Rgb) => `#${c.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
const mix = (a: Rgb, b: Rgb, t: number): Rgb => a.map((v, i) => Math.round(v + (b[i]! - v) * t)) as Rgb;
const lum = (c: Rgb) => {
  const l = c.map((v) => (v / 255 <= 0.03928 ? v / 255 / 12.92 : ((v / 255 + 0.055) / 1.055) ** 2.4));
  return 0.2126 * l[0]! + 0.7152 * l[1]! + 0.0722 * l[2]!;
};
const contrast = (a: Rgb, b: Rgb) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x! + 0.05) / (y! + 0.05);
};
const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];
/** `fc_core::theme::readable_accent`. */
function readable(accent: Rgb, bg: Rgb, scheme: Scheme): Rgb {
  const toward = scheme === 'light' ? BLACK : WHITE;
  for (let step = 0; step <= 20; step++) {
    const c = mix(accent, toward, step * 0.05);
    if (contrast(c, bg) >= 3) return c;
  }
  return toward;
}
const textOn = (fill: Rgb) => (contrast(fill, BLACK) >= contrast(fill, WHITE) ? BLACK : WHITE);

/** `fc_core::theme::tokens`. */
function tokens(b: Base, scheme: Scheme, custom: string | null): Record<string, string> {
  const bg = parse(b[0]);
  const accent = custom ? readable(parse(custom), bg, scheme) : parse(b[6]);
  const focus = custom ? accent : parse(b[7] || b[6]);
  return {
    bg: b[0],
    'bg-subtle': b[1],
    surface: b[2],
    text: b[3],
    'text-muted': b[4],
    border: b[5],
    accent: hex(accent),
    'accent-text': hex(textOn(accent)),
    danger: b[8],
    success: b[9],
    focus: hex(focus),
    selected: hex(mix(accent, bg, scheme === 'light' ? 0.86 : 0.72)),
  };
}

const LD = ['#c0392b', '#2e7d4f'] as const;
const DD = ['#ef6f62', '#6cc28d'] as const;
const BASES: { id: string; light: Base; dark: Base; fixed?: Record<Scheme, Record<string, string>> }[] = [
  // The default preset as tokens.css declares it (the v1.2 design; its hand-tuned accent text
  // and selection colours are kept unless a custom accent replaces them).
  {
    id: 'default',
    fixed: { light: { 'accent-text': '#17130a', selected: '#fef5e5' }, dark: { 'accent-text': '#17130a', selected: '#342b1e' } },
    light: ['#fafaf8', '#f3f3f0', '#ffffff', '#1a1a1c', '#5c5c63', '#e2e2dc', '#f5b544', '#9a5b00', '#c8323b', '#147a4b'],
    dark: ['#0e0e10', '#1c1c20', '#151518', '#ededef', '#a3a3ae', '#2a2a31', '#f5b544', '', '#ff6369', '#3dd68c'],
  },
  {
    id: 'paper',
    light: ['#fbf7ef', '#f3ecdf', '#fffdf8', '#2b2620', '#6b6155', '#e4dacb', '#9a5b2e', '', ...LD],
    dark: ['#1d1a16', '#26221c', '#26221c', '#ece4d6', '#a89d8c', '#3a342b', '#d9a066', '', ...DD],
  },
  {
    id: 'sage',
    light: ['#f7f9f6', '#eef2ec', '#ffffff', '#1d241f', '#5b675e', '#dbe3d8', '#3f7a5a', '', ...LD],
    dark: ['#141916', '#1b221d', '#1b221d', '#e3ebe5', '#9aa89e', '#2e3a32', '#7cc19a', '', ...DD],
  },
  {
    id: 'slate',
    light: ['#f6f8fa', '#edf1f5', '#ffffff', '#1b2430', '#56636f', '#d9e0e8', '#3b6ea8', '', ...LD],
    dark: ['#12161b', '#1a2027', '#1a2027', '#e2e8ef', '#94a1b0', '#2c3540', '#7fa8d9', '', ...DD],
  },
  {
    id: 'contrast',
    light: ['#ffffff', '#f0f0f0', '#ffffff', '#000000', '#333333', '#595959', '#0047b3', '', '#a30000', '#006b2e'],
    dark: ['#000000', '#121212', '#000000', '#ffffff', '#dddddd', '#a6a6a6', '#8ab4ff', '', '#ff8a80', '#7ee2a8'],
  },
];

const resolve = (p: (typeof BASES)[number], scheme: Scheme, accent: string | null) => ({
  ...tokens(p[scheme], scheme, accent),
  ...(accent ? {} : p.fixed?.[scheme]),
});

const PRESETS: ThemePresetDto[] = BASES.map((p) => ({
  id: p.id,
  name_key: `themes.preset.${p.id}`,
  light: resolve(p, 'light', null),
  dark: resolve(p, 'dark', null),
}));

export default {
  modules: [module('themes', [settingsPage('themes.appearance', 'settings.pages.appearance', '/settings/appearance', 1)])],
  handlers: (env) => {
    let mode: ThemeMode = 'system';
    let base = BASES[0]!;
    let accent: string | null = null;
    const current = (): ThemeDto => ({
      mode,
      preset: base.id,
      accent,
      light: resolve(base, 'light', accent),
      dark: resolve(base, 'dark', accent),
    });
    return {
      'themes.current': () => current(),
      'themes.presets': () => ({ presets: PRESETS }),
      'themes.set': (req) => {
        if (req.mode) mode = req.mode;
        if (req.preset) base = BASES.find((p) => p.id === req.preset) ?? base;
        if (req.accent !== null && req.accent !== undefined) accent = req.accent === '' ? null : req.accent.toLowerCase();
        env.emit({ type: 'setting_changed', data: { scope: { kind: 'app' }, key: 'themes.preset' } });
        return current();
      },
    };
  },
} satisfies MockFeature;
