/**
 * Dev-only mock of the v1.0 `themes` module. Tokens here are fixed samples so the settings can
 * be demoed without Rust; the real presets, accent adjustment and contrast rules live in
 * `fc_core::theme`.
 */
import { module, settingsPage, type MockFeature } from '$lib/api/mock/env';
import type { ThemeDto, ThemeMode, ThemePresetDto } from '$lib/api/types';

const tokens = (bg: string, text: string, accent: string): Record<string, string> => ({
  bg,
  'bg-subtle': bg,
  surface: bg,
  text,
  'text-muted': text,
  border: text,
  accent,
  'accent-text': '#ffffff',
  danger: '#c0392b',
  success: '#2e7d4f',
  focus: accent,
  selected: bg,
});

const PRESETS: ThemePresetDto[] = [
  // Mirrors fc_core::theme's default preset (v1.1, docs/11 §4) so mock screenshots match the app.
  {
    id: 'default',
    name_key: 'themes.preset.default',
    light: {
      bg: '#f6f6f8', 'bg-subtle': '#eeeef2', surface: '#ffffff', text: '#18181b', 'text-muted': '#5f5f6b', border: '#e4e4e9',
      accent: '#5b47e0', 'accent-text': '#ffffff', danger: '#c03636', success: '#157a53', focus: '#5b47e0', selected: '#e0def5',
    },
    dark: {
      bg: '#121214', 'bg-subtle': '#18181c', surface: '#1e1e23', text: '#ededf0', 'text-muted': '#a1a1aa', border: '#2e2e35',
      accent: '#8b7cf6', 'accent-text': '#000000', danger: '#ef6f62', success: '#3ec28f', focus: '#8b7cf6', selected: '#343053',
    },
  },
  { id: 'sage', name_key: 'themes.preset.sage', light: tokens('#f7f9f6', '#1d241f', '#3f7a5a'), dark: tokens('#141916', '#e3ebe5', '#7cc19a') },
];

export default {
  modules: [module('themes', [settingsPage('themes.appearance', 'settings.pages.appearance', '/settings/appearance', 1)])],
  handlers: (env) => {
    let mode: ThemeMode = 'system';
    let preset = PRESETS[0]!;
    let accent: string | null = null;
    const current = (): ThemeDto => {
      const pick = (t: Record<string, string>) => (accent ? { ...t, accent, focus: accent } : { ...t });
      return { mode, preset: preset.id, accent, light: pick(preset.light), dark: pick(preset.dark) };
    };
    return {
      'themes.current': () => current(),
      'themes.presets': () => ({ presets: PRESETS }),
      'themes.set': (req) => {
        if (req.mode) mode = req.mode;
        if (req.preset) preset = PRESETS.find((p) => p.id === req.preset) ?? preset;
        if (req.accent !== null && req.accent !== undefined) accent = req.accent === '' ? null : req.accent.toLowerCase();
        env.emit({ type: 'setting_changed', data: { scope: { kind: 'app' }, key: 'themes.preset' } });
        return current();
      },
    };
  },
} satisfies MockFeature;
