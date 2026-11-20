import {
  DEFAULT_LOCALE,
  LOCALES,
  detectLocale,
  interpolate,
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

/**
 * Texts that arrive at runtime with their own translations (plugins, v0.7: their locale
 * files, under `plugin.<id>.*`). Reactive: setting them re-renders what shows them.
 */
const runtime = $state<Record<Locale, Record<string, string>>>({ tr: {}, en: {} });

/** Replaces the runtime texts whose key starts with `prefix` for `locale`. */
export function setRuntimeMessages(locale: Locale, prefix: string, messages: Record<string, string>): void {
  const next: Record<string, string> = {};
  for (const [k, v] of Object.entries(runtime[locale])) if (!k.startsWith(prefix)) next[k] = v;
  runtime[locale] = Object.assign(next, messages);
}

/** For keys that come from the backend at runtime. */
export function td(key: string, params?: Params): string {
  const extra = runtime[state.locale][key] ?? runtime.en[key];
  if (extra !== undefined) return interpolate(extra, params);
  return translateDynamic(state.locale, key, params);
}
