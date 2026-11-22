/**
 * Settings search (v1.3, presentation only): the query typed above the settings
 * sub-navigation. `FormRow`s whose label, description or example contain every word light
 * up; `SettingsAdvanced` opens while a query is active so matching rows are never hidden.
 * Nothing here talks to the backend.
 */
const state = $state<{ query: string }>({ query: '' });

export const settingsSearch = {
  get query(): string {
    return state.query;
  },
  set query(q: string) {
    state.query = q;
  },
  /** A query is being typed (at least two characters, so one keystroke does not flash). */
  get active(): boolean {
    return words(state.query).length > 0 && state.query.trim().length >= 2;
  },
};

/** Lower-cases for both languages (Turkish dotted/dotless i) and folds accents. */
export function fold(s: string): string {
  return s
    .toLocaleLowerCase('tr')
    .replace(/ı/g, 'i')
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '');
}

export function words(q: string): string[] {
  return fold(q)
    .split(/\s+/)
    .filter((w) => w.length > 0);
}

/** Whether every word of `query` occurs in one of `texts`. Empty query: false. */
export function matchesQuery(query: string, ...texts: (string | null | undefined)[]): boolean {
  const ws = words(query);
  if (ws.length === 0) return false;
  const hay = fold(texts.filter(Boolean).join(' \u0000 '));
  return ws.every((w) => hay.includes(w));
}
