/**
 * The deck language of the word index (v1.4, docs/03 §4.2): `words.language` in deck scope
 * (then app scope; BCP-47, empty = unknown). A note whose language is unknown is not counted
 * among the known words; `words.stats.unassigned_notes` says how many. These helpers only read
 * and write that setting through the API and format it; the rules live in fc-app.
 */
import { api } from '$lib/api';
import type { DeckId, DeckTreeNode, SettingScope } from '$lib/api/types';
import { localeTag } from '$lib/format';

export const LANGUAGE_KEY = 'words.language';

/** Offered in the deck page's picker (the setting is free text; any stored tag is kept). */
export const COMMON_LANGUAGES = ['en', 'tr', 'de', 'es', 'fr', 'it'] as const;

export const deckScope = (id: DeckId): SettingScope => ({ kind: 'deck', id });

/** The language the index uses for the deck's notes ("" = unknown). */
export async function deckLanguage(id: DeckId): Promise<string> {
  const r = await api.call('settings.get', { scope: deckScope(id), keys: [LANGUAGE_KEY] });
  const v = r.values.find((x) => x.key === LANGUAGE_KEY)?.value;
  return typeof v === 'string' ? v.trim() : '';
}

/** Gives the deck a language; "" drops the deck's own value (back to the app's, if any). */
export function setDeckLanguage(id: DeckId, language: string): Promise<unknown> {
  return language
    ? api.call('settings.set', { scope: deckScope(id), key: LANGUAGE_KEY, value: language })
    : api.call('settings.reset', { scope: deckScope(id), key: LANGUAGE_KEY });
}

/** Readable name of a language tag in the UI language ("en" → "İngilizce" / "English"). */
export function languageName(tag: string, locale?: string): string {
  try {
    return new Intl.DisplayNames([localeTag(locale)], { type: 'language' }).of(tag) ?? tag;
  } catch {
    return tag;
  }
}

/**
 * The first deck (tree order) with cards and no language: where the Library's hint leads.
 * Asked only when the hint is clicked; null when every deck has one (the app scope's may be
 * missing then, or the unknown notes have a capture language that was cleared).
 */
export async function firstDeckWithoutLanguage(roots: DeckTreeNode[]): Promise<DeckId | null> {
  const flat = (ns: DeckTreeNode[]): DeckTreeNode[] => ns.flatMap((n) => [n, ...flat(n.children)]);
  const nodes = flat(roots);
  const langs = await Promise.all(nodes.map((n) => deckLanguage(n.deck.id).catch(() => null)));
  const missing = nodes.filter((_, i) => langs[i] === '');
  return (missing.find((n) => n.counts.total > 0) ?? missing[0])?.deck.id ?? null;
}
