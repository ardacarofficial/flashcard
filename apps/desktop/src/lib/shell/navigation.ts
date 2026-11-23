/**
 * Information architecture (docs/11 §2): the ONE place that says where each module's screen
 * lives — a sidebar section, the Inbox, or a Settings group. v1.4: sections and Settings
 * groups are either basic or shown only with the one "Advanced" switch (`advanced.svelte.ts`).
 *
 * Menus still come from `modules.list` (CLAUDE.md rule 7): a screen shows up only when an
 * enabled module contributes it (`main_menu` route, or a `settings_page`), so a module that
 * is switched off disappears from every section, group and the command palette. This file
 * only *places* what the backend lists; it decides nothing about behaviour.
 *
 * A route missing from {@link PLACEMENTS} (a new module, a plugin view) still shows up:
 * plugin views under Library, anything else in the Settings group "More" — never lost.
 */
import type { ModuleDto } from '$lib/api/types';
import type { MessageKey } from '$lib/i18n/index.svelte';
import type { ScreenDef } from '$lib/features';

export type SectionId = 'home' | 'library' | 'words' | 'create' | 'search' | 'stats';
export type SettingsGroupId = 'basic' | 'study' | 'app' | 'connections' | 'data' | 'other';

/** Where an entry is shown. */
export type Place = { kind: 'section'; id: SectionId } | { kind: 'settings'; id: SettingsGroupId } | { kind: 'inbox' };

const section = (id: SectionId): Place => ({ kind: 'section', id });
const settings = (id: SettingsGroupId): Place => ({ kind: 'settings', id });

/**
 * v1.4 A/B "Gezinme" (prototype `olustur-ara-istatistik.html`, question 1): `A` = 4 places
 * (Home, My words, Progress, Settings; decks are a filter inside My words), `B` = the same
 * plus a separate "Decks" place. The prototype's first-load variant is A; flip it here.
 */
export const NAV_VARIANT = 'A' as 'A' | 'B';

/**
 * The sidebar's sections, in order (docs/11 §2). `icon`: an `iconFor` key. `basic`: shown
 * while the "Advanced" switch is off (v1.4: the few places a daily user needs); the others
 * appear with it on, and are always reachable from Ctrl K and the pages that link to them.
 * Create needs no place of its own: the top bar's "Add" opens it.
 */
export const SECTIONS: readonly { id: SectionId; label_key: MessageKey; icon: string; basic: boolean }[] = [
  { id: 'home', label_key: 'shell.section.home', icon: 'home', basic: true },
  { id: 'library', label_key: 'shell.section.library', icon: 'library', basic: NAV_VARIANT === 'B' },
  { id: 'words', label_key: 'shell.section.words', icon: 'book-open', basic: true },
  { id: 'create', label_key: 'shell.section.create', icon: 'plus', basic: false },
  { id: 'search', label_key: 'shell.section.search', icon: 'search', basic: false },
  { id: 'stats', label_key: 'shell.section.stats', icon: 'chart', basic: true },
];

/**
 * Settings groups, in order (v1.4, prototype `ayarlar.html`): "Basic" holds the few pages
 * everyone needs, each named after the question it answers; the rest sits in groups that
 * show only with the "Advanced" switch on (`advanced`). Search finds every page either way.
 */
export const SETTINGS_GROUPS: readonly { id: SettingsGroupId; label_key: MessageKey; advanced: boolean }[] = [
  { id: 'basic', label_key: 'shell.group.basic', advanced: false },
  { id: 'study', label_key: 'shell.group.study', advanced: true },
  { id: 'app', label_key: 'shell.group.app', advanced: true },
  { id: 'connections', label_key: 'shell.group.connections', advanced: true },
  { id: 'data', label_key: 'shell.group.data', advanced: true },
  { id: 'other', label_key: 'shell.group.other', advanced: true },
];

/**
 * Route (`main_menu` route, trailing `/*` = prefix) → primary place, plus extra places it is
 * also reachable from (`also`, e.g. AI also under Create). Pages within a place are shown in
 * this list's order (ties, e.g. several plugin views: the backend's `order`). `label_key`
 * renames a page where it is placed (v1.4: Basic settings pages are named for the user).
 */
export const PLACEMENTS: readonly { route: string; place: Place; also?: Place[]; tab?: false; label_key?: MessageKey }[] = [
  // Library (Advanced; v1.4 A: decks are a filter in My words): decks and notes as tabs; the
  // study modes live here too, opened from the study launcher (`tab: false`: no section tab).
  { route: '/decks', place: section('library') },
  { route: '/notes', place: section('library') },
  { route: '/study', place: section('library'), tab: false },
  { route: '/learn', place: section('library'), tab: false },
  { route: '/test', place: section('library'), tab: false },
  { route: '/match', place: section('library'), tab: false },
  { route: '/reading', place: section('library'), tab: false },
  { route: '/plugins/view/*', place: section('library') },
  // My words (v1.4, E3's `words` screen).
  { route: '/words', place: section('words') },
  // Create: add cards (one, bulk, paste, file), generate with AI; starter decks also here.
  { route: '/notes/new', place: section('create') },
  { route: '/ai', place: section('create'), also: [settings('connections')] },
  // Search and Progress (statistics).
  { route: '/browse', place: section('search') },
  { route: '/stats', place: section('stats') },
  { route: '/planning', place: section('stats') },
  { route: '/goals', place: section('stats') },
  // Inbox (sidebar footer).
  { route: '/inbox', place: { kind: 'inbox' } },
  // Basic settings: the everyday questions (prototype order).
  { route: '/settings/pace', place: settings('basic'), label_key: 'shell.page.pace' },
  { route: '/notetypes', place: settings('basic'), label_key: 'shell.page.cards' },
  { route: '/reminders', place: settings('basic'), label_key: 'shell.page.reminders' },
  { route: '/subtitle', place: settings('basic'), label_key: 'shell.page.subtitle' },
  { route: '/io', place: settings('basic'), label_key: 'shell.page.backup' },
  { route: '/settings/*', place: settings('basic'), label_key: 'shell.page.look' },
  // Advanced › Studying: the study screen and modes, words.
  { route: '/presets', place: settings('study') },
  { route: '/mode-settings', place: settings('study') },
  { route: '/settings/words', place: settings('study') },
  // Advanced › App.
  { route: '/shortcuts', place: settings('app') },
  { route: '/settings/adding', place: settings('app') },
  { route: '/profiles', place: settings('app') },
  // Advanced › Connections (details).
  { route: '/mcp', place: settings('connections') },
  { route: '/local-api', place: settings('connections') },
  { route: '/automation', place: settings('connections') },
  { route: '/plugins', place: settings('connections') },
  // Advanced › Data and upkeep.
  { route: '/starters', place: settings('data'), also: [section('create')] },
  { route: '/maintenance', place: settings('data') },
  { route: '/app-maintenance', place: settings('data') },
];

const routeMatches = (pattern: string, route: string) =>
  pattern.endsWith('/*') ? route.startsWith(pattern.slice(0, -1)) : pattern === route;

/** Placement of a route (exact patterns win over prefixes); `rank` = position in PLACEMENTS. */
export function placementOf(route: string): { place: Place; also: Place[]; rank: number; tab: boolean; label_key?: MessageKey } {
  const hit =
    PLACEMENTS.find((p) => !p.route.endsWith('/*') && p.route === route) ??
    PLACEMENTS.find((p) => p.route.endsWith('/*') && routeMatches(p.route, route));
  if (hit) return { place: hit.place, also: hit.also ?? [], rank: PLACEMENTS.indexOf(hit), tab: hit.tab !== false, label_key: hit.label_key };
  return { place: settings('other'), also: [], rank: PLACEMENTS.length, tab: true };
}

/** Whether a place shows only with the "Advanced" switch on (v1.4). */
export function isAdvancedPlace(p: Place): boolean {
  if (p.kind === 'section') return !(SECTIONS.find((s) => s.id === p.id)?.basic ?? false);
  if (p.kind === 'settings') return SETTINGS_GROUPS.find((g) => g.id === p.id)?.advanced ?? true;
  return false;
}

/** One reachable page. */
export interface NavEntry {
  /** Unique: the contribution id (`@<place>` suffix for extra places). */
  id: string;
  label_key: string;
  /** Backend icon key (`iconFor`). */
  icon: string | null;
  route: string | null;
  screen: ScreenDef;
  /** Backend order (`UiContribution.order`). */
  order: number;
  /** Position in PLACEMENTS (sort key before `order`). */
  rank: number;
  place: Place;
  /** `false` for an extra (`also`) place: the palette lists only primary entries. */
  primary: boolean;
  /** Shown as a tab of its section (study modes are not: the launcher opens them). */
  tab: boolean;
  /** Its place shows only with the "Advanced" switch on (v1.4). */
  advanced: boolean;
}

export interface Navigation {
  /** Sections in sidebar order with their pages (Home always; others when non-empty). */
  sections: { id: SectionId; label_key: MessageKey; icon: string; basic: boolean; entries: NavEntry[] }[];
  // (`entries` holds every page of the section; the shell draws tabs for those with `tab`.)
  /** Non-empty Settings groups in order (Advanced ones included: the layout hides them). */
  settings: { id: SettingsGroupId; label_key: MessageKey; advanced: boolean; entries: NavEntry[] }[];
  inbox: NavEntry | null;
  /** Every primary entry (command palette, deep links). */
  all: NavEntry[];
}

export const samePlace = (a: Place, b: Place) => a.kind === b.kind && (a.kind === 'inbox' || a.id === (b as { id: string }).id);

/**
 * The sections the sidebar shows (v1.4): with "Advanced" off only the basic ones; Search
 * stands in for My words while no `words` screen is registered (its module off or not built),
 * so finding a card never needs the switch.
 */
export function visibleSections(nav: Navigation, advancedOn: boolean): Navigation['sections'] {
  if (advancedOn) return nav.sections;
  const hasWords = nav.sections.some((s) => s.id === 'words');
  return nav.sections.filter((s) => s.basic || (s.id === 'search' && !hasWords));
}

/**
 * Builds the navigation from `modules.list` and the registered screens. `home` is the
 * shell's own Home page (always present: it is not a module).
 */
export function buildNavigation(modules: ModuleDto[], screens: ScreenDef[], home: ScreenDef): Navigation {
  const enabled = modules.filter((m) => m.enabled);
  const ui = enabled.flatMap((m) => m.ui);
  const screenOf = (route: string) => screens.find((s) => s.routes?.some((p) => routeMatches(p, route))) ?? null;
  const entry = (e: Omit<NavEntry, 'advanced'>): NavEntry => ({ ...e, advanced: isAdvancedPlace(e.place) });
  const primary: NavEntry[] = [
    entry({ id: 'home', label_key: 'shell.section.home', icon: 'home', route: null, screen: home, order: 0, rank: -1, place: section('home'), primary: true, tab: true }),
  ];
  for (const c of ui) {
    if (c.slot !== 'main_menu' || !c.route) continue;
    const s = screenOf(c.route);
    if (!s) continue;
    const { place, rank, tab, label_key } = placementOf(c.route);
    primary.push(entry({ id: c.id, label_key: label_key ?? c.label_key, icon: c.icon, route: c.route, screen: s, order: c.order, rank, place, primary: true, tab }));
  }
  // Screens opened by a slot (the Settings page for `settings_page` contributions).
  for (const s of screens) {
    if (!s.slot) continue;
    const slotted = ui.filter((c) => c.slot === s.slot?.name).sort((a, b) => a.order - b.order);
    const first = slotted[0];
    if (!first) continue;
    const at = first.route ? placementOf(first.route) : { place: settings('basic'), rank: 0, label_key: undefined };
    // One entry for all of them (the page shows every contribution): named after the first.
    primary.push(entry({ id: s.id, label_key: at.label_key ?? first.label_key, icon: 'settings', route: null, screen: s, order: first.order, rank: at.rank, place: at.place, primary: true, tab: true }));
  }
  // v1.4: the shell's own settings pages (rows of several modules' settings): shown while one
  // of the modules they draw from is on, so a switched-off module leaves no dead page.
  for (const s of screens) {
    const sp = s.settingsPage;
    if (!sp || !sp.modules.some((id) => enabled.some((m) => m.id === id))) continue;
    const { place, rank } = placementOf(sp.route);
    primary.push(entry({ id: s.id, label_key: sp.label_key, icon: 'settings', route: sp.route, screen: s, order: 0, rank, place, primary: true, tab: true }));
  }
  const extra: NavEntry[] = primary.flatMap((e) =>
    e.route
      ? placementOf(e.route).also.map((place) => entry({ ...e, id: `${e.id}@${place.kind}.${'id' in place ? place.id : ''}`, place, primary: false }))
      : [],
  );
  const byRank = (a: NavEntry, b: NavEntry) => a.rank - b.rank || a.order - b.order;
  const every = [...primary, ...extra].sort(byRank);
  const at = (p: Place) => every.filter((e) => samePlace(e.place, p));

  return {
    sections: SECTIONS.map((s) => ({ ...s, entries: at(section(s.id)) })).filter((s) => s.entries.length > 0),
    settings: SETTINGS_GROUPS.map((g) => ({ ...g, entries: at(settings(g.id)) })).filter((g) => g.entries.length > 0),
    inbox: at({ kind: 'inbox' })[0] ?? null,
    all: primary.sort(byRank),
  };
}
