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
  { id: 'default', name_key: 'themes.preset.default', light: tokens('#ffffff', '#1c1917', '#2f6fdb'), dark: tokens('#161514', '#ecebe9', '#6b9bf0') },
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
