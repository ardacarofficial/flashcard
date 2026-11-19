/**
 * DEV-ONLY in-memory backend so the UI is demoable in a plain browser
 * (`pnpm dev:mock`) and testable without Rust. It is NOT a source of truth for
 * behaviour: the real rules live in fc-app. Loaded only when
 * `import.meta.env.DEV` (see ../index.ts) and by tests. Speaks the real wire
 * protocol: qualified `v1.*` names, generated request/response types, `ApiError`
 * DTOs on failure and `EventEnvelope`s on the event stream.
 */
import { API_VERSION, type MethodName, type RequestOf, type ResponseOf } from '../generated/methods';
import type {
  ApiError,
  AttributeDefDto,
  DeckDto,
  DeckTreeNode,
  DomainEvent,
  ErrorCode,
  EventEnvelope,
  ModuleDto,
  NoteDto,
  UiContribution,
} from '../types';
import type { Transport } from '../transport';

function err(code: ErrorCode, message_key: string, message: string): ApiError {
  return { code, message_key, message, details: {}, retryable: false };
}

const CEFR = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'];

function menu(id: string, label_key: string, route: string, order: number): UiContribution {
  return { slot: 'main_menu', id, label_key, icon: null, route, method: null, order };
}

function settingsPage(id: string, label_key: string, route: string, order: number): UiContribution {
  return { slot: 'settings_page', id, label_key, icon: null, route, method: null, order };
}

function module(id: string, ui: UiContribution[]): ModuleDto {
  return {
    id,
    name_key: `modules.${id}.name`,
    description_key: `modules.${id}.description`,
    enabled: true,
    can_disable: id !== 'core',
    ui,
    methods: [],
  };
}

type Handlers = { [M in MethodName]?: (req: RequestOf<M>) => ResponseOf<M> };

export function createMockTransport(): Transport {
  let seq = 0;
  let eventSeq = 0;
  const now = () => ++seq;
  const id = (p: string) => `${p}-${(++seq).toString().padStart(4, '0')}`;
  const listeners = new Set<(e: EventEnvelope) => void>();
  const emit = (event: DomainEvent) => {
    const env: EventEnvelope = { seq: ++eventSeq, at: now(), batch_id: null, event };
    for (const l of listeners) l(structuredClone(env));
  };

  const decks: DeckDto[] = [];
  const pathOf = (d: DeckDto): string => {
    const parent = decks.find((p) => p.id === d.parent_id);
    return parent ? `${pathOf(parent)}::${d.name}` : d.name;
  };
  const addDeck = (name: string, parent: DeckDto | null): DeckDto => {
    const t = now();
    const d: DeckDto = {
      id: id('deck'),
      parent_id: parent?.id ?? null,
      name,
      path: parent ? `${parent.path}::${name}` : name,
      description: '',
      icon: null,
      color: null,
      preset_id: null,
      position: decks.filter((x) => x.parent_id === (parent?.id ?? null)).length,
      created_at: t,
      updated_at: t,
    };
    decks.push(d);
    return d;
  };
  const english = addDeck('İngilizce', null);
  addDeck('Fiiller', english);
  addDeck('YKS', null);

  const attrs: AttributeDefDto[] = [
    {
      id: id('attr'),
      scope_deck_id: english.id,
      key: 'level',
      name: 'Seviye',
      kind: 'ordered_enum',
      options: CEFR.map((label, i) => ({ id: `opt-${label}`, label, color: null, order: i })),
      target: 'note',
      required: false,
      default_value: null,
      position: 0,
      template_id: 'cefr',
      updated_at: 0,
    },
  ];
  const notes: NoteDto[] = [];

  const tree = (parent: string | null): DeckTreeNode[] =>
    decks
      .filter((d) => d.parent_id === parent)
      .sort((a, b) => a.position - b.position || a.name.localeCompare(b.name))
      .map((deck) => ({
        deck: { ...deck, path: pathOf(deck) },
        counts: { new: 0, learning: 0, due: 0, total: 0 },
        children: tree(deck.id),
      }));

  const ancestors = (deckId: string): string[] => {
    const out: string[] = [];
    let cur = decks.find((d) => d.id === deckId);
    while (cur) {
      out.push(cur.id);
      cur = decks.find((d) => d.id === cur?.parent_id);
    }
    return out;
  };

  const requireDeck = (deckId: string) => {
    const d = decks.find((x) => x.id === deckId);
    if (!d) throw err('NOT_FOUND_DECK', 'errors.not_found.deck', deckId);
    return d;
  };
  const requireName = (name: string) => {
    const n = name.trim();
    if (n === '' || n.includes('::')) {
      throw err('VALIDATION_DECK_NAME', 'errors.validation.deck_name', name);
    }
    return n;
  };
  const sameName = (a: string, b: string) => a.toLocaleLowerCase() === b.toLocaleLowerCase();

  const handlers: Handlers = {
    'modules.list': () => ({
      modules: [
        module('core', [settingsPage('core.general', 'settings.pages.general', '/settings/general', 0)]),
        module('decks', [menu('decks.tree', 'nav.decks', '/decks', 10)]),
        module('notes', [
          menu('notes.add', 'nav.add_note', '/notes/new', 20),
          menu('notes.list', 'nav.notes', '/notes', 30),
        ]),
      ],
    }),
    'decks.tree': () => ({ roots: tree(null) }),
    'decks.create': ({ path, parent_id }) => {
      const parts = path.split('::').map(requireName);
      let parent: DeckDto | null = parent_id ? requireDeck(parent_id) : null;
      for (const [i, name] of parts.entries()) {
        const existing: DeckDto | undefined = decks.find(
          (d) => d.parent_id === (parent?.id ?? null) && sameName(d.name, name),
        );
        if (existing && i === parts.length - 1) {
          throw err('CONFLICT_DECK_EXISTS', 'errors.conflict.deck_exists', path);
        }
        parent = existing ?? addDeck(name, parent);
        if (!existing) emit({ type: 'deck_created', data: { deck: parent } });
      }
      return { ...parent!, path: pathOf(parent!) };
    },
    'decks.rename': ({ id: deckId, name }) => {
      const d = requireDeck(deckId);
      d.name = requireName(name);
      d.updated_at = now();
      const dto = { ...d, path: pathOf(d) };
      emit({ type: 'deck_updated', data: { deck: dto } });
      return dto;
    },
    'decks.delete': ({ id: target }) => {
      requireDeck(target);
      const doomed = new Set(decks.filter((d) => ancestors(d.id).includes(target)).map((d) => d.id));
      for (let i = decks.length - 1; i >= 0; i--) if (doomed.has(decks[i]!.id)) decks.splice(i, 1);
      let removed = 0;
      for (let i = notes.length - 1; i >= 0; i--) {
        if (doomed.has(notes[i]!.home_deck_id)) {
          notes.splice(i, 1);
          removed++;
        }
      }
      const affected = { decks: doomed.size, notes: removed, cards: removed };
      emit({ type: 'deck_deleted', data: { id: target, affected } });
      return affected;
    },
    'attributes.effective': ({ deck_id, target }) => {
      const chain = ancestors(deck_id);
      return {
        attributes: attrs
          .filter((a) => a.target === target)
          .filter((a) => a.scope_deck_id === null || chain.includes(a.scope_deck_id))
          .map((def) => ({
            def,
            inherited: def.scope_deck_id !== deck_id,
            suggested_value: null,
          })),
      };
    },
    'notes.create': (req) => {
      requireDeck(req.deck_id);
      if (!req.fields.front?.trim()) {
        throw err('VALIDATION_FIELD_REQUIRED', 'errors.validation.field_required', 'front');
      }
      const t = now();
      const n: NoteDto = {
        id: id('note'),
        notetype_id: req.notetype_id,
        home_deck_id: req.deck_id,
        fields: req.fields,
        tags: req.tags,
        attributes: req.attributes,
        cards: [
          {
            id: id('card'),
            deck_id: req.deck_id,
            template_key: 'forward',
            state: 'new',
            due: null,
            suspended: false,
          },
        ],
        sort_field: req.fields.front,
        created_at: t,
        updated_at: t,
      };
      notes.push(n);
      emit({ type: 'note_created', data: { note_id: n.id, deck_id: req.deck_id } });
      return n;
    },
    'notes.list': ({ deck_id }) => ({
      items: notes
        .filter((n) => !deck_id || n.home_deck_id === deck_id)
        .map((n) => ({
          id: n.id,
          notetype_id: n.notetype_id,
          sort_field: n.sort_field,
          tags: n.tags,
          card_count: n.cards.length,
          updated_at: n.updated_at,
        })),
      next_cursor: null,
    }),
    'notes.delete': ({ ids }) => {
      const gone = ids.filter((noteId) => {
        const i = notes.findIndex((n) => n.id === noteId);
        if (i >= 0) notes.splice(i, 1);
        return i >= 0;
      });
      if (gone.length === 0) {
        throw err('NOT_FOUND_NOTE', 'errors.not_found.note', ids.join(','));
      }
      emit({ type: 'notes_deleted', data: { ids: gone } });
      return { decks: 0, notes: gone.length, cards: gone.length };
    },
  };

  return {
    async invoke(command, payload) {
      const prefix = `${API_VERSION}.`;
      const method = (command.startsWith(prefix) ? command.slice(prefix.length) : '') as MethodName;
      const h = handlers[method] as ((req: unknown) => unknown) | undefined;
      if (!h) throw err('NOT_FOUND_METHOD', 'errors.not_found.method', command);
      // Deep-copy in and out so callers can't mutate mock state (mirrors IPC serialisation).
      return structuredClone(h(structuredClone(payload ?? {})));
    },
    async listen(handler) {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  };
}
