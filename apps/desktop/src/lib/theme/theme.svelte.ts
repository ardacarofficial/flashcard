export type ThemeMode = 'system' | 'light' | 'dark';
export const THEME_MODES: readonly ThemeMode[] = ['system', 'light', 'dark'];

const STORAGE_KEY = 'fc.theme';

function initial(): ThemeMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v && (THEME_MODES as readonly string[]).includes(v)) return v as ThemeMode;
  } catch {
    /* storage unavailable */
  }
  return 'system';
}

/** "system" removes the attribute so the prefers-color-scheme media query decides. */
export function applyTheme(mode: ThemeMode, root: HTMLElement = document.documentElement): void {
  if (mode === 'system') delete root.dataset.theme;
  else root.dataset.theme = mode;
}

// TODO(v0.1 integration): persist via `settings.set` once the backend exposes it.
const state = $state<{ mode: ThemeMode }>({ mode: initial() });

export const theme = {
  get mode(): ThemeMode {
    return state.mode;
  },
  set mode(m: ThemeMode) {
    state.mode = m;
    try {
      localStorage.setItem(STORAGE_KEY, m);
    } catch {
      /* ignore */
    }
    applyTheme(m);
  },
  init(): void {
    applyTheme(state.mode);
  },
};
