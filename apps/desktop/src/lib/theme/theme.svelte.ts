/**
 * Theme state (docs/02 §31). The backend owns the choice and the colours (`themes.current`:
 * scheme, preset, accent and both schemes' tokens, computed in fc-core); this file only
 * applies them: `data-theme` for the scheme, `--fc-<token>: light-dark(<light>, <dark>)` on
 * the root for the colours. `tokens.css` holds the default preset (the v1.2 design), used
 * for it, before the backend answers and when the themes module is off.
 *
 * The last applied theme is cached in localStorage so the next start paints the right colours
 * at once (a per-viewer convenience; the settings stay the source of truth).
 */
import { api } from '$lib/api';
import type { SetTheme, ThemeDto, ThemeMode } from '$lib/api/types';

export type { ThemeMode };
export const THEME_MODES: readonly ThemeMode[] = ['system', 'light', 'dark'];

const CACHE_KEY = 'fc.theme.v2';

/** "system" removes the attribute so the prefers-color-scheme media query decides. */
export function applyTheme(mode: ThemeMode, root: HTMLElement = document.documentElement): void {
  if (mode === 'system') delete root.dataset.theme;
  else root.dataset.theme = mode;
}

/** Token names set by the last {@link applyTokens} (removed when tokens go away). */
let applied: string[] = [];

/** The preset whose colours are the v1.2 design in `tokens.css` (fc_core `DEFAULT_PRESET`). */
export const DEFAULT_PRESET = 'default';
/** Backend tokens that follow the accent (all a custom accent changes on the default preset). */
const ACCENT_TOKENS = ['accent', 'accent-text', 'focus', 'selected'];

type Scheme = Record<string, string>;
/** `color-mix` of two scheme tokens (`a` weighted `pct` %), or `null` when one is missing. */
const mix = (t: Scheme, a: string, pct: number, b: string) => (t[a] && t[b] ? `color-mix(in srgb, ${t[a]} ${pct}%, ${t[b]})` : null);

/**
 * Layer-1 tokens tokens.css declares exactly for the default preset and that the backend does
 * not send: derived from a preset's base colours so the design system's steps exist for
 * every preset. Accent and success as text are moved towards the text colour until they read
 * as text on every surface (AA 4.5:1 for any accent the backend allows, which only promises
 * 3:1 against the background; checked in tests/contrast.test.ts).
 */
const DERIVED: Record<string, (t: Scheme, s: 'light' | 'dark') => string | null> = {
  'surface-3': (t) => mix(t, 'bg-subtle', 94, 'text'),
  'border-strong': (t) => mix(t, 'border', 91, 'text'),
  'text-3': (t) => mix(t, 'text-muted', 70, 'bg'),
  'success-ink': (t) => mix(t, 'success', 85, 'text'),
  'accent-ink': (t, s) => mix(t, 'accent', s === 'light' ? 60 : 50, 'text'),
};
const ACCENT_DERIVED = ['accent-ink'];

/**
 * Sets `--fc-<token>` for every token of both schemes; `null` falls back to `tokens.css`.
 * The default preset keeps tokens.css (the v1.2 design) and only takes the accent tokens of a
 * custom accent from the backend; other presets get all their tokens plus the derived ones.
 */
export function applyTokens(
  theme: (Pick<ThemeDto, 'light' | 'dark'> & Partial<Pick<ThemeDto, 'preset' | 'accent'>>) | null,
  root: HTMLElement = document.documentElement,
): void {
  for (const name of applied) root.style.removeProperty(`--fc-${name}`);
  applied = [];
  if (!theme) return;
  const own = theme.preset === DEFAULT_PRESET;
  if (own && !theme.accent) return;
  const set = (name: string, light: string, dark: string) => {
    root.style.setProperty(`--fc-${name}`, `light-dark(${light}, ${dark})`);
    applied.push(name);
  };
  for (const [name, light] of Object.entries(theme.light)) {
    if (own && !ACCENT_TOKENS.includes(name)) continue;
    set(name, light, theme.dark[name] ?? light);
  }
  for (const [name, derive] of Object.entries(DERIVED)) {
    if (own && !ACCENT_DERIVED.includes(name)) continue;
    const light = derive(theme.light, 'light');
    const dark = derive(theme.dark, 'dark');
    if (light && dark) set(name, light, dark);
  }
}

function cached(): ThemeDto | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    return raw ? (JSON.parse(raw) as ThemeDto) : null;
  } catch {
    return null;
  }
}

function remember(t: ThemeDto | null): void {
  try {
    if (t) localStorage.setItem(CACHE_KEY, JSON.stringify(t));
    else localStorage.removeItem(CACHE_KEY);
  } catch {
    /* storage unavailable */
  }
}

const state = $state<{ current: ThemeDto | null; mode: ThemeMode }>({ current: null, mode: 'system' });

function show(t: ThemeDto | null, mode: ThemeMode): void {
  state.current = t;
  state.mode = mode;
  applyTheme(mode);
  applyTokens(t);
}

/** The scheme stored in `app.theme` (used when the themes module is off). */
async function storedMode(): Promise<ThemeMode> {
  const res = await api.call('settings.get', { scope: { kind: 'app' }, keys: ['app.theme'] });
  const v = res.values.find((s) => s.key === 'app.theme')?.value;
  return v === 'light' || v === 'dark' ? v : 'system';
}

let started = false;

/** `PERMISSION_APP_LOCKED`: the call was refused by the app lock (not a missing module). */
function isLocked(e: unknown): boolean {
  return typeof e === 'object' && e !== null && (e as { code?: unknown }).code === 'PERMISSION_APP_LOCKED';
}

export const theme = {
  /** Effective theme from the backend (`null` = themes module off or not loaded yet). */
  get current(): ThemeDto | null {
    return state.current;
  },
  get mode(): ThemeMode {
    return state.mode;
  },
  /** Paints the cached theme synchronously (before the first render). */
  init(): void {
    const c = cached();
    show(c, c?.mode ?? 'system');
  },
  /** Re-reads the theme from the backend and applies it. */
  async sync(): Promise<void> {
    try {
      const t = await api.call('themes.current', {});
      show(t, t.mode);
      remember(t);
    } catch (e) {
      // Locked with the themes module off (`settings.get` is behind the lock): keep the
      // cached colours on the lock screen; `lock_changed` syncs again after unlocking.
      if (isLocked(e)) return;
      // Themes module off: default colours, the scheme still comes from `app.theme`.
      try {
        show(null, await storedMode());
      } catch (e2) {
        if (isLocked(e2)) return;
        show(null, 'system');
      }
      remember(null);
    }
  },
  /** Loads the theme and follows changes (settings, undo/redo, module switches). Idempotent. */
  start(): void {
    if (started) return;
    started = true;
    void this.sync();
    void api.on(['setting_changed', 'module_toggled', 'undone', 'redone', 'data_changed', 'lock_changed'], () => void this.sync());
  },
  /** Changes scheme, preset and/or accent (one undo step in the backend). */
  async set(change: SetTheme): Promise<void> {
    if (state.current) {
      const t = await api.call('themes.set', change);
      show(t, t.mode);
      remember(t);
    } else if (change.mode) {
      await api.call('settings.set', { scope: { kind: 'app' }, key: 'app.theme', value: change.mode });
      show(null, change.mode);
    }
  },
};
