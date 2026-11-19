/**
 * Dev-only mock of the browse/palette modules (see `api/mock/transport.ts`). Deliberately
 * naive: free words match the front text, every `key:value` filter is ignored. The real
 * query language lives in fc-core/fc-storage.
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import { METHODS, type MethodName } from '$lib/api/generated/methods';
import type { CardRow, SavedQueryDto, ShortcutDto } from '$lib/api/types';

const DEFAULTS: [string, ShortcutDto['context'], string | null][] = [
  ['ui.palette', 'global', 'Ctrl+K'],
  ['ui.search', 'global', 'Ctrl+F'],
  ['ui.new_note', 'global', 'Ctrl+N'],
  ['ui.undo', 'global', 'Ctrl+Z'],
  ['ui.redo', 'global', 'Ctrl+Shift+Z'],
  ['browse.select_all', 'browse', 'Ctrl+A'],
  ['browse.suspend', 'browse', 'Ctrl+J'],
  ['browse.info', 'browse', 'Ctrl+I'],
  ['browse.delete', 'browse', 'Delete'],
];

const DIRECT = new Set<MethodName>(['oplog.undo', 'oplog.redo', 'decks.tree', 'browse.check_database']);

export default {
  modules: [
    module('browse', [menu('browse.cards', 'nav.browse', '/browse', 35), menu('browse.maintenance', 'nav.maintenance', '/maintenance', 90)]),
    module('palette', [menu('palette.shortcuts', 'nav.shortcuts', '/shortcuts', 95)]),
  ],
  handlers: ({ now, id, emit, notes, decks, attrs, pathOf }: MockEnv) => {
    const saved: SavedQueryDto[] = [];
    const overrides = new Map<string, string | null>();
    const rows = (query: string): CardRow[] => {
      const words = query
        .split(/\s+/)
        .filter((w) => w && !w.includes(':'))
        .map((w) => w.toLocaleLowerCase('tr'));
      return notes
        .filter((n) => words.every((w) => (n.fields.front ?? '').toLocaleLowerCase('tr').includes(w)))
        .flatMap((n) =>
          n.cards.map((c) => {
            const deck = decks.find((d) => d.id === c.deck_id);
            return {
              card_id: c.id,
              note_id: n.id,
              deck_id: c.deck_id,
              deck_path: deck ? pathOf(deck) : '',
              notetype_id: n.notetype_id,
              sort_field: n.sort_field,
              template_key: c.template_key,
              card_type: 'classic',
              state: c.state,
              due: c.due,
              stability: null,
              difficulty: null,
              reps: 0,
              lapses: 0,
              flags: 0,
              suspended: c.suspended,
              buried: false,
              tags: n.tags,
              created_at: n.created_at,
              note_updated_at: n.updated_at,
            };
          }),
        );
    };
    const shortcuts = () => ({
      items: [
        ...DEFAULTS.map(([action, context, def]) => ({
          action,
          label_key: `shortcuts.${action}`,
          context,
          combo: overrides.has(action) ? overrides.get(action)! : def,
          default_combo: def,
        })),
      ],
      conflicts: [],
    });
    const done = (n: number) => ({ changed: n, skipped: 0 });
    return {
      // Read-only; the attributes feature owns the real handlers.
      'attributes.list': () => ({ defs: attrs }),
      'search.cards': ({ query, offset, limit }) => {
        const all = rows(query);
        const from = offset ?? 0;
        return { items: all.slice(from, from + (limit ?? 100)), total: all.length, offset: from };
      },
      'search.ids': ({ query }) => {
        const all = rows(query);
        return { card_ids: all.map((r) => r.card_id), note_ids: [...new Set(all.map((r) => r.note_id))] };
      },
      'search.check': ({ query }) => {
        const quotes = (query.match(/"/g) ?? []).length;
        if (quotes % 2 === 1) {
          const start = query.lastIndexOf('"');
          return { error: { kind: 'unclosed_quote', span: { start, end: query.length }, params: {} }, normalized: null };
        }
        return { error: null, normalized: query.trim() };
      },
      'saved_queries.list': () => ({ items: saved }),
      'saved_queries.create': ({ name, query, icon }) => {
        if (!name.trim()) throw err('VALIDATION_SAVED_QUERY_NAME', 'errors.validation.saved_query_name', name);
        const q: SavedQueryDto = { id: id('sq'), name: name.trim(), query, icon: icon ?? null, position: saved.length, created_at: now(), updated_at: now() };
        saved.push(q);
        emit({ type: 'saved_queries_changed' });
        return q;
      },
      'saved_queries.delete': ({ id: qid }) => {
        const i = saved.findIndex((q) => q.id === qid);
        if (i < 0) throw err('NOT_FOUND_SAVED_QUERY', 'errors.not_found.saved_query', qid);
        const [q] = saved.splice(i, 1);
        emit({ type: 'saved_queries_changed' });
        return q!;
      },
      'bulk.move_cards': ({ card_ids }) => done(card_ids.length),
      'bulk.set_suspended': ({ card_ids }) => done(card_ids.length),
      'bulk.set_buried': ({ card_ids }) => done(card_ids.length),
      'bulk.set_flag': ({ card_ids }) => done(card_ids.length),
      'bulk.reset_progress': ({ card_ids }) => done(card_ids.length),
      'bulk.add_tags': ({ note_ids }) => done(note_ids.length),
      'bulk.remove_tags': ({ note_ids }) => done(note_ids.length),
      'bulk.find_replace': ({ note_ids }) => done(note_ids.length),
      'bulk.set_attribute': ({ card_ids }) => done(card_ids.length),
      'browse.trash': () => ({ decks: [], notes: [] }),
      'browse.duplicates': () => ({ groups: [] }),
      'browse.check_database': () => ({
        orphan_cards: [],
        cards_in_trashed_decks: [],
        unscheduled_cards: [],
        missing_search_index: [],
        cardless_notes: [],
        dangling_values: 0,
        healthy: true,
      }),
      'palette.list': () => ({
        entries: (Object.keys(METHODS) as MethodName[]).sort().map((m) => ({
          method: `v1.${m}`,
          kind: METHODS[m],
          module: m.split('.')[0]!,
          label_key: `commands.${m}`,
          direct: DIRECT.has(m),
          enabled: true,
          shortcut: null,
        })),
      }),
      'shortcuts.list': () => shortcuts(),
      'shortcuts.set': ({ action, combo }) => {
        overrides.set(action, combo);
        emit({ type: 'setting_changed', data: { scope: { kind: 'app' }, key: 'palette.shortcuts' } });
        return shortcuts();
      },
      'shortcuts.reset': ({ action }) => {
        if (action) overrides.delete(action);
        else overrides.clear();
        return shortcuts();
      },
    };
  },
} satisfies MockFeature;
