import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { ModuleDto, UiContribution } from '$lib/api/types';
import { SCREENS, type ScreenDef } from '$lib/features';
import { buildNavigation, placementOf, SECTIONS, visibleSections } from './navigation';

const HOME: ScreenDef = { id: 'home', load: async () => ({ default: (() => {}) as never }) };

const menu = (id: string, route: string, order: number): UiContribution => ({
  slot: 'main_menu',
  id,
  label_key: `nav.${id}`,
  icon: null,
  route,
  method: null,
  order,
});
const mod = (id: string, ui: UiContribution[], enabled = true): ModuleDto => ({
  id,
  name_key: '',
  description_key: '',
  enabled,
  can_disable: true,
  ui,
  methods: [],
});

/** Every `main_menu(…, "/route", …)` and `settings_page(…, "/route", …)` the Rust modules register. */
function rustRoutes(): string[] {
  const dir = join(__dirname, '..', '..', '..', '..', '..', 'crates', 'fc-app', 'src', 'modules');
  const files = (d: string): string[] =>
    readdirSync(d).flatMap((n) => (statSync(join(d, n)).isDirectory() ? files(join(d, n)) : n.endsWith('.rs') ? [join(d, n)] : []));
  const out = new Set<string>();
  for (const f of files(dir)) {
    const text = readFileSync(f, 'utf8');
    for (const m of text.matchAll(/(?:main_menu|settings_page)\(\s*"[^"]+",\s*"[^"]+",\s*(?:"[^"]+",\s*)?"(\/[^"]*)"/g)) out.add(m[1]!);
  }
  return [...out].sort();
}

describe('navigation (docs/11 §2)', () => {
  it('places every route the backend registers explicitly (none falls into "More")', () => {
    const routes = rustRoutes();
    expect(routes.length).toBeGreaterThan(25);
    const unplaced = routes.filter((r) => {
      const p = placementOf(r).place;
      return p.kind === 'settings' && p.id === 'other';
    });
    expect(unplaced).toEqual([]);
    // …and each has a screen to open.
    const screenless = routes.filter((r) => !r.startsWith('/settings/') && !SCREENS.some((s) => s.routes?.some((p) => (p.endsWith('/*') ? r.startsWith(p.slice(0, -1)) : p === r))));
    expect(screenless).toEqual([]);
  });

  it('builds the sections, Home first and always there (v1.4: + My words)', () => {
    const nav = buildNavigation([], SCREENS, HOME);
    expect(nav.sections.map((s) => s.id)).toEqual(['home']);
    expect(nav.all.map((e) => e.id)).toEqual(['home']);
    expect(SECTIONS.map((s) => s.id)).toEqual(['home', 'library', 'words', 'create', 'search', 'stats']);
  });

  it('v1.4: the sidebar shows the basic places; the rest with "Advanced" on', () => {
    const modules = [
      mod('decks', [menu('decks.tree', '/decks', 10)]),
      mod('notes', [menu('notes.add', '/notes/new', 20)]),
      mod('browse', [menu('browse.cards', '/browse', 35)]),
      mod('stats', [menu('stats.overview', '/stats', 40)]),
    ];
    const nav = buildNavigation(modules, SCREENS, HOME);
    // No `words` screen registered yet: Search stands in for My words.
    expect(visibleSections(nav, false).map((s) => s.id)).toEqual(['home', 'search', 'stats']);
    expect(visibleSections(nav, true).map((s) => s.id)).toEqual(['home', 'library', 'create', 'search', 'stats']);
    // A registered My words screen takes the place (route id `words`, E3).
    const WORDS: ScreenDef = { id: 'words', routes: ['/words'], load: async () => ({ default: (() => {}) as never }) };
    const withWords = buildNavigation([...modules, mod('words', [menu('words.list', '/words', 30)])], [...SCREENS, WORDS], HOME);
    expect(visibleSections(withWords, false).map((s) => s.id)).toEqual(['home', 'words', 'stats']);
    expect(withWords.sections.find((s) => s.id === 'stats')?.label_key).toBe('shell.section.stats');
  });

  it('a switched-off module disappears everywhere (sections, settings, extra places, palette)', () => {
    const modules = [
      mod('decks', [menu('decks.tree', '/decks', 10)]),
      mod('onboarding', [menu('onboarding.starters', '/starters', 94)]),
      mod('browse', [menu('browse.cards', '/browse', 35)]),
    ];
    const on = buildNavigation(modules, SCREENS, HOME);
    expect(on.sections.map((s) => s.id)).toEqual(['home', 'library', 'create', 'search']);
    expect(on.settings.map((g) => [g.id, g.entries.map((e) => e.id)])).toEqual([['data', ['onboarding.starters']]]);
    expect(on.settings[0]!.advanced).toBe(true);
    // Starter decks are also reachable from Create (an extra place), but listed once in the
    // palette. (v1.3: Import / export left Create — importing is part of "Add cards".)
    expect(on.sections.find((s) => s.id === 'create')?.entries.map((e) => e.id)).toEqual(['onboarding.starters@section.create']);
    expect(on.all.filter((e) => e.route === '/starters')).toHaveLength(1);

    const off = buildNavigation([modules[0]!, { ...modules[1]!, enabled: false }, modules[2]!], SCREENS, HOME);
    expect(off.sections.map((s) => s.id)).toEqual(['home', 'library', 'search']);
    expect(off.settings).toEqual([]);
    expect(off.all.some((e) => e.route === '/starters')).toBe(false);
  });

  it('v1.4: Backup is a Basic settings page, named for the user (importing is part of Add cards)', () => {
    const nav = buildNavigation([mod('io', [menu('io.main', '/io', 60)]), mod('notes', [menu('notes.add', '/notes/new', 20)])], SCREENS, HOME);
    expect(nav.settings.map((g) => [g.id, g.entries.map((e) => e.id)])).toEqual([['basic', ['io.main']]]);
    expect(nav.settings[0]!.entries[0]!.label_key).toBe('shell.page.backup');
    expect(nav.sections.find((s) => s.id === 'create')?.entries.map((e) => e.id)).toEqual(['notes.add']);
  });

  it('orders pages as navigation.ts lists them; inbox is its own place; unknown routes go to "More"', () => {
    const nav = buildNavigation(
      [mod('m', [menu('study', '/study', 5), menu('decks', '/decks', 10), menu('inbox', '/inbox', 25), menu('x', '/plugins/view/p/v', 1000), menu('new', '/new-thing', 1)])],
      SCREENS,
      HOME,
    );
    expect(nav.sections.find((s) => s.id === 'library')?.entries.map((e) => e.id)).toEqual(['decks', 'study', 'x']);
    expect(nav.inbox?.id).toBe('inbox');
    // `/new-thing` has no screen: not shown at all.
    expect(nav.all.some((e) => e.id === 'new')).toBe(false);
    expect(placementOf('/new-thing').place).toEqual({ kind: 'settings', id: 'other' });
  });

  it('settings_page contributions open the Settings page under Basic ("Language and look")', () => {
    const nav = buildNavigation(
      [mod('core', [{ slot: 'settings_page', id: 'core.general', label_key: 'settings.pages.general', icon: null, route: '/settings/general', method: null, order: 0 }])],
      SCREENS,
      HOME,
    );
    expect(nav.settings.map((g) => [g.id, g.entries.map((e) => e.screen.id)])).toEqual([['basic', ['settings']]]);
    expect(nav.settings[0]!.entries[0]!.label_key).toBe('shell.page.look');
  });

  it('v1.4: the shell\'s own settings pages show while one of their modules is on', () => {
    const on = buildNavigation([mod('words', [])], SCREENS, HOME);
    expect(on.settings.map((g) => [g.id, g.entries.map((e) => e.route)])).toEqual([['study', ['/settings/words']]]);
    const off = buildNavigation([mod('words', [], false)], SCREENS, HOME);
    expect(off.settings).toEqual([]);
  });
});

describe('section tabs (v1.1)', () => {
  it('Library shows Decks | Notes as tabs; the study modes are reachable but not tabs', () => {
    const modules = [
      mod('decks', [menu('decks.tree', '/decks', 10)]),
      mod('notes', [menu('notes.list', '/notes', 11)]),
      mod('study', [menu('study.session', '/study', 5), menu('study.mode_settings', '/mode-settings', 90)]),
      mod('match', [menu('study.match', '/match', 8)]),
    ];
    const nav = buildNavigation(modules, SCREENS, HOME);
    const lib = nav.sections.find((s) => s.id === 'library')!;
    expect(lib.entries.filter((e) => e.tab).map((e) => e.route)).toEqual(['/decks', '/notes']);
    expect(lib.entries.filter((e) => !e.tab).map((e) => e.route)).toEqual(['/study', '/match']);
    // Their settings live in Settings › Advanced › "Studying".
    expect(nav.settings.find((g) => g.id === 'study')?.entries.map((e) => e.route)).toEqual(['/mode-settings']);
  });
});

describe('settings groups (v1.4)', () => {
  const route = (id: string, r: string, order = 50) => mod(id, [menu(id, r, order)]);
  it('Basic pages first (the everyday questions); every other group is Advanced', () => {
    const routes = ['/reminders', '/shortcuts', '/profiles', '/presets', '/notetypes', '/mode-settings', '/ai', '/subtitle', '/io', '/starters', '/automation', '/mcp', '/local-api', '/plugins', '/maintenance', '/app-maintenance'];
    const nav = buildNavigation(
      // `planning`: the Daily pace page (v1.4) writes through it, so it shows while it is on.
      [...routes.map((r, i) => route(`m${i}`, r, 100 - i)), route('planning', '/planning')],
      SCREENS,
      HOME,
    );
    // v1.4: Basic starts with the simple Daily pace page; the full preset editor moved to
    // Advanced › Studying (nothing removed).
    expect(nav.settings.map((g) => [g.id, g.entries.map((e) => e.route)])).toEqual([
      ['basic', ['/settings/pace', '/notetypes', '/reminders', '/subtitle', '/io']],
      ['study', ['/presets', '/mode-settings']],
      ['app', ['/shortcuts', '/profiles']],
      ['connections', ['/ai', '/mcp', '/local-api', '/automation', '/plugins']],
      ['data', ['/starters', '/maintenance', '/app-maintenance']],
    ]);
    expect(nav.settings.map((g) => g.advanced)).toEqual([false, true, true, true, true]);
    expect(nav.settings[0]!.entries.length).toBeLessThanOrEqual(6);
    // Fewer groups than pages, and nothing lost: every page is reachable from Settings once.
    expect(nav.settings.flatMap((g) => g.entries).filter((e) => e.primary || e.route === '/ai')).toHaveLength(routes.length + 1);
  });
});
