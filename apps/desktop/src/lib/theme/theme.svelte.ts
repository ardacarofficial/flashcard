/**
 * Theme state (docs/02 §31). The backend owns the choice and the colours (`themes.current`:
 * scheme, preset, accent and both schemes' tokens, computed in fc-core); this file only
 * applies them: `data-theme` for the scheme, `--fc-<token>: light-dark(<light>, <dark>)` on
 * the root for the colours. `tokens.css` holds the same default values, used before the
 * backend answers and when the themes module is off.
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

/** Sets `--fc-<token>` for every token of both schemes; `null` falls back to `tokens.css`. */
export function applyTokens(
  theme: Pick<ThemeDto, 'light' | 'dark'> | null,
  root: HTMLElement = document.documentElement,
): void {
  for (const name of applied) root.style.removeProperty(`--fc-${name}`);
  applied = [];
  if (!theme) return;
  for (const [name, light] of Object.entries(theme.light)) {
    const dark = theme.dark[name] ?? light;
    root.style.setProperty(`--fc-${name}`, `light-dark(${light}, ${dark})`);
    applied.push(name);
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
