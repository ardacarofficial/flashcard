import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { ModuleDto, UiContribution } from '$lib/api/types';
import { SCREENS, type ScreenDef } from '$lib/features';
import { buildNavigation, placementOf, SECTIONS } from './navigation';

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

  it('builds at most the 5 sections, Home first and always there', () => {
    const nav = buildNavigation([], SCREENS, HOME);
    expect(nav.sections.map((s) => s.id)).toEqual(['home']);
    expect(nav.all.map((e) => e.id)).toEqual(['home']);
    expect(SECTIONS.map((s) => s.id)).toEqual(['home', 'library', 'create', 'search', 'stats']);
  });

  it('a switched-off module disappears everywhere (sections, settings, extra places, palette)', () => {
    const modules = [
      mod('decks', [menu('decks.tree', '/decks', 10)]),
      mod('io', [menu('io.main', '/io', 60)]),
      mod('browse', [menu('browse.cards', '/browse', 35)]),
    ];
    const on = buildNavigation(modules, SCREENS, HOME);
    expect(on.sections.map((s) => s.id)).toEqual(['home', 'library', 'create', 'search']);
    expect(on.settings.map((g) => [g.id, g.entries.map((e) => e.id)])).toEqual([['data', ['io.main']]]);
    // Import is also reachable from Create (an extra place), but listed once in the palette.
    expect(on.sections.find((s) => s.id === 'create')?.entries.map((e) => e.id)).toEqual(['io.main@section.create']);
    expect(on.all.filter((e) => e.route === '/io')).toHaveLength(1);

    const off = buildNavigation([modules[0]!, { ...modules[1]!, enabled: false }, modules[2]!], SCREENS, HOME);
    expect(off.sections.map((s) => s.id)).toEqual(['home', 'library', 'search']);
    expect(off.settings).toEqual([]);
    expect(off.all.some((e) => e.route === '/io')).toBe(false);
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

  it('settings_page contributions open the Settings page under General', () => {
    const nav = buildNavigation(
      [mod('core', [{ slot: 'settings_page', id: 'core.general', label_key: 'settings.pages.general', icon: null, route: '/settings/general', method: null, order: 0 }])],
      SCREENS,
      HOME,
    );
    expect(nav.settings.map((g) => [g.id, g.entries.map((e) => e.screen.id)])).toEqual([['general', ['settings']]]);
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
    // Their settings live in Settings → "Reading and modes".
    expect(nav.settings.find((g) => g.id === 'reading')?.entries.map((e) => e.route)).toEqual(['/mode-settings']);
  });
});
