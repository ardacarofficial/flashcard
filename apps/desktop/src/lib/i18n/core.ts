// Merged at build time from `locales/<lang>/<feature>.json` by `scripts/merge-locales.mjs`
// (run by `pnpm check|test|build|dev`). Edit the feature files, never these.
import en from './generated/en.json';
import tr from './generated/tr.json';

/** EN is the reference dictionary; every other locale must have the same shape. */
export type Dictionary = typeof en;
export type Locale = 'tr' | 'en';
export const LOCALES: readonly Locale[] = ['tr', 'en'];
export const DEFAULT_LOCALE: Locale = 'tr';

type Join<P extends string, K extends string> = P extends '' ? K : `${P}.${K}`;
type Paths<T, P extends string = ''> = {
  [K in keyof T & string]: T[K] extends string ? Join<P, K> : Paths<T[K], Join<P, K>>;
}[keyof T & string];

/** Every valid translation key, e.g. "deck.new". Typos fail `pnpm check`. */
export type MessageKey = Paths<Dictionary>;
export type Params = Record<string, string | number>;

// Compile-time guarantee that TR is not missing any key EN has.
const trChecked: Dictionary = tr;
export const DICTIONARIES: Record<Locale, Dictionary> = { en, tr: trChecked };

function lookup(dict: unknown, key: string): string | undefined {
  let cur: unknown = dict;
  for (const part of key.split('.')) {
    if (typeof cur !== 'object' || cur === null) return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}

export function interpolate(s: string, params?: Params): string {
  if (!params) return s;
  return s.replace(/\{(\w+)\}/g, (m, name: string) =>
    name in params ? String(params[name]) : m,
  );
}

/** Typed lookup. Falls back to EN, then to the key itself. */
export function translate(locale: Locale, key: MessageKey, params?: Params): string {
  return translateDynamic(locale, key, params);
}

/**
 * Lookup for keys that arrive at runtime from the backend
 * (module `name_key`, menu `label_key`, error `message_key`).
 */
export function translateDynamic(locale: Locale, key: string, params?: Params): string {
  const s = lookup(DICTIONARIES[locale], key) ?? lookup(DICTIONARIES.en, key) ?? key;
  return interpolate(s, params);
}

/** All leaf keys of a dictionary (used by tests to compare locales). */
export function flatKeys(dict: unknown, prefix = ''): string[] {
  if (typeof dict !== 'object' || dict === null) return [prefix];
  return Object.entries(dict as Record<string, unknown>).flatMap(([k, v]) =>
    flatKeys(v, prefix ? `${prefix}.${k}` : k),
  );
}

export function detectLocale(languages: readonly string[]): Locale {
  for (const l of languages) {
    const base = l.toLowerCase().split('-')[0];
    if (base === 'tr' || base === 'en') return base;
  }
  return DEFAULT_LOCALE;
}
