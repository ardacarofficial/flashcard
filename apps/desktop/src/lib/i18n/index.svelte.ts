import {
  DEFAULT_LOCALE,
  LOCALES,
  detectLocale,
  translate,
  translateDynamic,
  type Locale,
  type MessageKey,
  type Params,
} from './core';

export { LOCALES, type Locale, type MessageKey } from './core';

const STORAGE_KEY = 'fc.locale';

function initial(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && (LOCALES as readonly string[]).includes(saved)) return saved as Locale;
  } catch {
    /* storage unavailable */
  }
  return typeof navigator !== 'undefined' ? detectLocale(navigator.languages) : DEFAULT_LOCALE;
}

// TODO(v0.1 integration): persist via `settings.set` (app scope) once the backend exposes it.
const state = $state<{ locale: Locale }>({ locale: initial() });

export const i18n = {
  get locale(): Locale {
    return state.locale;
  },
  set locale(l: Locale) {
    state.locale = l;
    try {
      localStorage.setItem(STORAGE_KEY, l);
    } catch {
      /* ignore */
    }
    if (typeof document !== 'undefined') document.documentElement.lang = l;
  },
};

/** Reactive typed translation: re-renders when the locale changes. */
export function t(key: MessageKey, params?: Params): string {
  return translate(state.locale, key, params);
}

/** For keys that come from the backend at runtime. */
export function td(key: string, params?: Params): string {
  return translateDynamic(state.locale, key, params);
}
