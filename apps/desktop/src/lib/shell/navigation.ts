/**
 * Information architecture (docs/11 §2): the ONE place that says where each module's screen
 * lives — one of the 5 sidebar sections, the Inbox, or a Settings group.
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

export type SectionId = 'home' | 'library' | 'create' | 'search' | 'stats';
export type SettingsGroupId = 'general' | 'study' | 'connections' | 'data' | 'advanced' | 'other';

/** Where an entry is shown. */
export type Place = { kind: 'section'; id: SectionId } | { kind: 'settings'; id: SettingsGroupId } | { kind: 'inbox' };

const section = (id: SectionId): Place => ({ kind: 'section', id });
const settings = (id: SettingsGroupId): Place => ({ kind: 'settings', id });

/** The sidebar's 5 sections, in order (docs/11 §2). `icon`: an `iconFor` key. */
export const SECTIONS: readonly { id: SectionId; label_key: MessageKey; icon: string }[] = [
  { id: 'home', label_key: 'shell.section.home', icon: 'home' },
  { id: 'library', label_key: 'shell.section.library', icon: 'library' },
  { id: 'create', label_key: 'shell.section.create', icon: 'plus' },
  { id: 'search', label_key: 'shell.section.search', icon: 'search' },
  { id: 'stats', label_key: 'shell.section.stats', icon: 'chart' },
];

/**
 * Settings groups, in order (v1.3): by what the user is trying to do, most used first; the
 * technical destinations (MCP, local API, automation, plugins, maintenance, logs) sit apart
 * under "Advanced" (`hint_key`: the line the sub-navigation shows under its heading).
 */
export const SETTINGS_GROUPS: readonly { id: SettingsGroupId; label_key: MessageKey; hint_key?: MessageKey }[] = [
  { id: 'general', label_key: 'shell.group.general' },
  { id: 'study', label_key: 'shell.group.study' },
  { id: 'connections', label_key: 'shell.group.connections' },
  { id: 'data', label_key: 'shell.group.data' },
  { id: 'advanced', label_key: 'shell.group.advanced', hint_key: 'shell.group.advanced_hint' },
  { id: 'other', label_key: 'shell.group.other' },
];

/**
 * Route (`main_menu` route, trailing `/*` = prefix) → primary place, plus extra places it is
 * also reachable from (`also`, e.g. Import also under Create). Pages within a place are
 * shown in this list's order (ties, e.g. several plugin views: the backend's `order`).
 */
export const PLACEMENTS: readonly { route: string; place: Place; also?: Place[]; tab?: false }[] = [
  // Library: decks and notes as tabs; the study modes live here too, but are opened from the
  // deck page's mode tiles and Home's "Start studying" (`tab: false`: no section tab).
  { route: '/decks', place: section('library') },
  { route: '/notes', place: section('library') },
  { route: '/study', place: section('library'), tab: false },
  { route: '/learn', place: section('library'), tab: false },
  { route: '/test', place: section('library'), tab: false },
  { route: '/match', place: section('library'), tab: false },
  { route: '/reading', place: section('library'), tab: false },
  { route: '/plugins/view/*', place: section('library') },
  // Create: add cards, generate with AI; import and starter decks are also here.
  { route: '/notes/new', place: section('create') },
  { route: '/ai', place: section('create'), also: [settings('connections')] },
  // Search and Statistics.
  { route: '/browse', place: section('search') },
  { route: '/stats', place: section('stats') },
  { route: '/planning', place: section('stats') },
  { route: '/goals', place: section('stats') },
  // Inbox (sidebar footer).
  { route: '/inbox', place: { kind: 'inbox' } },
  // Settings groups (v1.3: by task, most used first; technical pages under "Advanced").
  // General: how the app looks and behaves for you.
  { route: '/settings/*', place: settings('general') },
  { route: '/reminders', place: settings('general') },
  { route: '/shortcuts', place: settings('general') },
  { route: '/profiles', place: settings('general') },
  // Studying and cards: how cards are scheduled, what they look like, the study modes.
  { route: '/presets', place: settings('study') },
  { route: '/notetypes', place: settings('study') },
  { route: '/mode-settings', place: settings('study') },
  // Connections: AI (also under Create) and the Subtitle browser extension.
  { route: '/subtitle', place: settings('connections') },
  // Data.
  { route: '/io', place: settings('data'), also: [section('create')] },
  { route: '/starters', place: settings('data'), also: [section('create')] },
  // Advanced: rarely needed, technical.
  { route: '/automation', place: settings('advanced') },
  { route: '/mcp', place: settings('advanced') },
  { route: '/local-api', place: settings('advanced') },
  { route: '/plugins', place: settings('advanced') },
  { route: '/maintenance', place: settings('advanced') },
  { route: '/app-maintenance', place: settings('advanced') },
];

const routeMatches = (pattern: string, route: string) =>
  pattern.endsWith('/*') ? route.startsWith(pattern.slice(0, -1)) : pattern === route;

/** Placement of a route (exact patterns win over prefixes); `rank` = position in PLACEMENTS. */
export function placementOf(route: string): { place: Place; also: Place[]; rank: number; tab: boolean } {
  const hit =
    PLACEMENTS.find((p) => !p.route.endsWith('/*') && p.route === route) ??
    PLACEMENTS.find((p) => p.route.endsWith('/*') && routeMatches(p.route, route));
  if (hit) return { place: hit.place, also: hit.also ?? [], rank: PLACEMENTS.indexOf(hit), tab: hit.tab !== false };
  return { place: settings('other'), also: [], rank: PLACEMENTS.length, tab: true };
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
  /** Shown as a tab of its section (study modes are not: the deck page opens them). */
  tab: boolean;
}

export interface Navigation {
  /** Visible sections in sidebar order with their pages (Home always; others when non-empty). */
  sections: { id: SectionId; label_key: MessageKey; icon: string; entries: NavEntry[] }[];
  // (`entries` holds every page of the section; the shell draws tabs for those with `tab`.)
  /** Non-empty Settings groups in order. */
  settings: { id: SettingsGroupId; label_key: MessageKey; entries: NavEntry[] }[];
  inbox: NavEntry | null;
  /** Every primary entry (command palette, deep links). */
  all: NavEntry[];
}

export const samePlace = (a: Place, b: Place) => a.kind === b.kind && (a.kind === 'inbox' || a.id === (b as { id: string }).id);

/**
 * Builds the navigation from `modules.list` and the registered screens. `home` is the
 * shell's own Home page (always present: it is not a module).
 */
export function buildNavigation(modules: ModuleDto[], screens: ScreenDef[], home: ScreenDef): Navigation {
  const ui = modules.filter((m) => m.enabled).flatMap((m) => m.ui);
  const screenOf = (route: string) => screens.find((s) => s.routes?.some((p) => routeMatches(p, route))) ?? null;
  const primary: NavEntry[] = [
    { id: 'home', label_key: 'shell.section.home', icon: 'home', route: null, screen: home, order: 0, rank: -1, place: section('home'), primary: true, tab: true },
  ];
  for (const c of ui) {
    if (c.slot !== 'main_menu' || !c.route) continue;
    const s = screenOf(c.route);
    if (!s) continue;
    const { place, rank, tab } = placementOf(c.route);
    primary.push({ id: c.id, label_key: c.label_key, icon: c.icon, route: c.route, screen: s, order: c.order, rank, place, primary: true, tab });
  }
  // Screens opened by a slot (the Settings page for `settings_page` contributions).
  for (const s of screens) {
    if (!s.slot) continue;
    const slotted = ui.filter((c) => c.slot === s.slot?.name).sort((a, b) => a.order - b.order);
    const first = slotted[0];
    if (!first) continue;
    const { place, rank } = first.route ? placementOf(first.route) : { place: settings('general'), rank: 0 };
    // One entry for all of them (the page shows every contribution): named after the first.
    primary.push({ id: s.id, label_key: first.label_key, icon: 'settings', route: null, screen: s, order: first.order, rank, place, primary: true, tab: true });
  }
  const extra: NavEntry[] = primary.flatMap((e) =>
    e.route ? placementOf(e.route).also.map((place) => ({ ...e, id: `${e.id}@${place.kind}.${'id' in place ? place.id : ''}`, place, primary: false })) : [],
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
