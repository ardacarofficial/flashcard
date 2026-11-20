// Merged at build time from `locales/<lang>/<feature>.json` by `scripts/merge-locales.mjs`
// (run by `pnpm check|test|build|dev`). Edit the feature files, never these.
//
// Only the *types* are imported statically: each language is its own lazily loaded chunk
// (`loadLocale`), so the initial bundle carries no dictionary at all and the app loads just
// the active language before mounting (switching language loads the other one).
import type en from './generated/en.json';
import type tr from './generated/tr.json';

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
type TrComplete = typeof tr extends Dictionary ? true : never;
export const TR_COMPLETE: TrComplete = true;

const LOADERS: Record<Locale, () => Promise<{ default: Dictionary }>> = {
  en: () => import('./generated/en.json'),
  tr: () => import('./generated/tr.json'),
};

/** The dictionaries loaded so far (filled by `loadLocale`). */
export const DICTIONARIES: Partial<Record<Locale, Dictionary>> = {};

const pending: Partial<Record<Locale, Promise<Dictionary>>> = {};

/** Loads `locale`'s dictionary once (a separate chunk); resolves when lookups can use it. */
export function loadLocale(locale: Locale): Promise<Dictionary> {
  const have = DICTIONARIES[locale];
  if (have) return Promise.resolve(have);
  return (pending[locale] ??= LOADERS[locale]().then((m) => (DICTIONARIES[locale] = m.default)));
}

export const isLocaleLoaded = (locale: Locale): boolean => DICTIONARIES[locale] !== undefined;

function lookup(dict: unknown, key: string): string | undefined {
  let cur: unknown = dict;
  for (const part of key.split('.')) {
    if (typeof cur !== 'object' || cur === null) return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}

const BCP: Record<string, string> = { tr: 'tr-TR', en: 'en-US' };
const rules = new Map<string, Intl.PluralRules>();

/** Index of the `}` closing the `{` at `open`. */
function closing(s: string, open: number): number {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    if (s[i] === '{') depth++;
    else if (s[i] === '}' && --depth === 0) return i;
  }
  return -1;
}

/**
 * `{name}` substitution plus ICU plurals: `{n, plural, =0 {none} one {# card} other {# cards}}`
 * (`#` = the number). Categories come from `Intl.PluralRules` of the locale (TR has only `other`).
 */
export function interpolate(s: string, params?: Params, locale: string = DEFAULT_LOCALE): string {
  if (!params) return s;
  let out = '';
  for (let i = 0; i < s.length; i++) {
    if (s[i] !== '{') {
      out += s[i];
      continue;
    }
    const end = closing(s, i);
    if (end < 0) {
      out += s.slice(i);
      break;
    }
    const body = s.slice(i + 1, end);
    const head = /^(\w+)\s*,\s*plural\s*,/.exec(body);
    const key = head?.[1];
    if (head && key !== undefined && key in params) {
      const n = Number(params[key]);
      const opts = new Map<string, string>();
      const re = /(=\d+|\w+)\s*\{/g;
      let rest = body.slice(head[0].length);
      for (let m = re.exec(rest); m; m = re.exec(rest)) {
        const o = m.index + m[0].length - 1;
        const c = closing(rest, o);
        if (c < 0) break;
        opts.set(m[1] ?? '', rest.slice(o + 1, c));
        re.lastIndex = c + 1;
      }
      const tag = BCP[locale] ?? locale;
      let pr = rules.get(tag);
      if (!pr) rules.set(tag, (pr = new Intl.PluralRules(tag)));
      const chosen = opts.get(`=${n}`) ?? opts.get(pr.select(n)) ?? opts.get('other') ?? '';
      out += interpolate(chosen.replace(/#/g, new Intl.NumberFormat(tag).format(n)), params, locale);
    } else if (/^\w+$/.test(body) && body in params) out += String(params[body]);
    else out += s.slice(i, end + 1);
    i = end;
  }
  return out;
}

/** Typed lookup. Falls back to EN (if loaded), then to the key itself. */
export function translate(locale: Locale, key: MessageKey, params?: Params): string {
  return translateDynamic(locale, key, params);
}

/**
 * Lookup for keys that arrive at runtime from the backend
 * (module `name_key`, menu `label_key`, error `message_key`).
 */
export function translateDynamic(locale: Locale, key: string, params?: Params): string {
  const s = lookup(DICTIONARIES[locale], key) ?? lookup(DICTIONARIES.en, key) ?? key;
  return interpolate(s, params, locale);
}

/** All leaf keys of a dictionary (used by tests to compare locales). */
export function flatKeys(dict: unknown, prefix = ''): string[] {
  if (typeof dict !== 'object' || dict === null) return [prefix];
  return Object.entries(dict as Record<string, unknown>).flatMap(([k, v]) =>
    flatKeys(v, prefix ? `${prefix}.${k}` : k),
  );
}

/**
 * The OS / browser language list: `navigator.languages` (WebView2 and WebKitGTK report the
 * system UI language there), then the `Intl` default locale, which follows the OS too.
 */
export function systemLanguages(): string[] {
  const out: string[] = [];
  if (typeof navigator !== 'undefined') out.push(...(navigator.languages ?? []), ...(navigator.language ? [navigator.language] : []));
  try {
    out.push(Intl.DateTimeFormat().resolvedOptions().locale);
  } catch {
    /* no Intl */
  }
  return out;
}

export function detectLocale(languages: readonly string[]): Locale {
  for (const l of languages) {
    const base = l.toLowerCase().split('-')[0];
    if (base === 'tr' || base === 'en') return base;
  }
  return DEFAULT_LOCALE;
}
