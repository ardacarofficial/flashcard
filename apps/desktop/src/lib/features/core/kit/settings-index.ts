/**
 * Words of each settings page for the settings search (v1.3, presentation only): the texts a
 * page shows come from its feature's i18n namespaces, so the index is those dictionary
 * subtrees (labels, descriptions, hints) — no list to keep in sync by hand. Error texts are
 * left out. A page without an entry here is still found by its own name.
 */
import { DICTIONARIES, type Locale } from '$lib/i18n/core';
import { fold, words } from './search.svelte';

/** Route (as in navigation.ts) → dictionary subtrees the page shows. */
export const PAGE_TEXTS: Record<string, readonly string[]> = {
  '/settings/general': ['settings.app', 'settings.general_intro', 'settings.guide', 'settings.themes', 'themes', 'lang'],
  '/shortcuts': ['shortcuts'],
  '/reminders': ['reminders', 'settings.reminders'],
  '/profiles': ['profiles', 'lock'],
  '/presets': ['presets'],
  '/notetypes': ['notetype', 'cardtypes'],
  '/mode-settings': ['mode_settings', 'settings.study', 'settings.learn', 'settings.practice_test', 'settings.confidence', 'settings.timed', 'settings.focus', 'settings.match', 'settings.reading', 'settings.tts'],
  '/ai': ['ai', 'settings.ai'],
  '/subtitle': ['subtitle', 'settings.subtitle'],
  '/io': ['io', 'settings.io'],
  '/starters': ['starters'],
  '/automation': ['automation', 'settings.automation'],
  '/mcp': ['mcp', 'settings.mcp'],
  '/local-api': ['local_api'],
  '/plugins': ['plugins', 'settings.plugins'],
  '/maintenance': ['maintenance', 'settings.maintenance'],
  '/app-maintenance': ['app_maintenance', 'settings.updates'],
  // v1.4: the shell's own pages of rows from several modules' settings.
  '/settings/words': ['settings_pages.words', 'settings.words'],
  '/settings/adding': ['settings_pages.adding', 'settings.quick_add', 'settings.clipboard', 'settings.attributes'],
};

const SKIP = new Set(['errors', 'commands']);

function node(dict: unknown, path: string): unknown {
  let cur = dict;
  for (const part of path.split('.')) {
    if (typeof cur !== 'object' || cur === null) return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return cur;
}

function leaves(n: unknown, out: string[]): void {
  if (typeof n === 'string') {
    // Dynamic messages ("{count} cards waiting") are status lines, not what a page is about.
    if (!n.includes('{')) out.push(n);
    return;
  }
  if (typeof n !== 'object' || n === null) return;
  for (const [k, v] of Object.entries(n)) if (!SKIP.has(k)) leaves(v, out);
}

const cache = new Map<string, { raw: string; folded: string }[]>();

/** The texts of the page at `route` in `locale` (empty when unknown or not loaded). */
export function pageTexts(route: string, locale: Locale): { raw: string; folded: string }[] {
  const id = `${locale}:${route}`;
  const hit = cache.get(id);
  if (hit) return hit;
  const dict = DICTIONARIES[locale];
  const prefixes = PAGE_TEXTS[route];
  if (!dict || !prefixes) return [];
  const out: string[] = [];
  for (const p of prefixes) leaves(node(dict, p), out);
  const texts = [...new Set(out)].map((raw) => ({ raw, folded: fold(raw) }));
  cache.set(id, texts);
  return texts;
}

/**
 * Whether a page matches `query`: its name, or one of its texts, contains every word.
 * `snippet` is the shortest such text (shown under the page name), `null` when the name
 * itself matched.
 */
export function matchPage(query: string, name: string, route: string | null, locale: Locale): { snippet: string | null } | null {
  const ws = words(query);
  if (ws.length === 0) return null;
  if (ws.every((w) => fold(name).includes(w))) return { snippet: null };
  let best: string | null = null;
  for (const x of route ? pageTexts(route, locale) : []) {
    if (ws.every((w) => x.folded.includes(w)) && (best === null || x.raw.length < best.length)) best = x.raw;
  }
  return best === null ? null : { snippet: best.trim() };
}
