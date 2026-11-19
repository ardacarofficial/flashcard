/**
 * DEV-ONLY in-memory backend so the UI is demoable before the Rust side is
 * wired. It is NOT a source of truth for behaviour: the real rules live in
 * fc-app. Loaded only when `import.meta.env.DEV` (see ../index.ts) and by tests.
 */
import type { ApiErrorDto, AttributeDef, DeckNode, FcEvent, NoteSummary } from '../types';
import type { Transport } from '../transport';

interface FlatDeck {
  id: string;
  parent_id: string | null;
  name: string;
}

function err(code: string, message_key: string, message: string): ApiErrorDto {
  return { code, message_key, message, details: null, retryable: false };
}

const CEFR = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'];

export function createMockTransport(): Transport {
  let seq = 0;
  const id = (p: string) => `${p}-${(++seq).toString().padStart(4, '0')}`;
  const listeners = new Set<(e: FcEvent) => void>();
  const emit = (type: string, payload: unknown) => {
    for (const l of listeners) l({ type, payload });
  };

  const english = { id: id('deck'), parent_id: null, name: 'İngilizce' };
  const decks: FlatDeck[] = [
    english,
    { id: id('deck'), parent_id: english.id, name: 'Fiiller' },
    { id: id('deck'), parent_id: null, name: 'YKS' },
  ];
  const attrs: AttributeDef[] = [
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
    },
  ];
  const notes: NoteSummary[] = [];

  const tree = (parent: string | null): DeckNode[] =>
    decks
      .filter((d) => d.parent_id === parent)
      .map((d) => ({ ...d, children: tree(d.id) }));

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
    if (!d) throw err('NOT_FOUND_DECK', 'errors.deck.not_found', deckId);
    return d;
  };
  const requireName = (name: string) => {
    if (name.trim() === '') throw err('VALIDATION_EMPTY_NAME', 'errors.validation.empty_name', '');
  };

  type Handler = (p: Record<string, unknown>) => unknown;
  const handlers: Record<string, Handler> = {
    'v1.modules.list': () => [
      {
        id: 'decks',
        name_key: 'modules.decks',
        enabled: true,
        ui_contributions: [
          { kind: 'menu_item', id: 'decks', label_key: 'menu.decks', screen: 'decks', position: 0 },
        ],
      },
      {
        id: 'notes',
        name_key: 'modules.notes',
        enabled: true,
        ui_contributions: [
          { kind: 'menu_item', id: 'add', label_key: 'menu.add_note', screen: 'add_note', position: 1 },
          { kind: 'menu_item', id: 'browse', label_key: 'menu.notes', screen: 'notes', position: 2 },
        ],
      },
      {
        id: 'settings',
        name_key: 'modules.settings',
        enabled: true,
        ui_contributions: [
          { kind: 'menu_item', id: 'settings', label_key: 'menu.settings', screen: 'settings', position: 9 },
        ],
      },
    ],
    'v1.deck.tree': () => tree(null),
    'v1.deck.create': (p) => {
      const name = String(p.name ?? '');
      requireName(name);
      const parent = (p.parent_id as string | null) ?? null;
      if (parent) requireDeck(parent);
      const d = { id: id('deck'), parent_id: parent, name: name.trim() };
      decks.push(d);
      emit('deck.created', { id: d.id });
      return { id: d.id };
    },
    'v1.deck.rename': (p) => {
      const d = requireDeck(String(p.id));
      const name = String(p.name ?? '');
      requireName(name);
      d.name = name.trim();
      emit('deck.renamed', { id: d.id });
      return null;
    },
    'v1.deck.delete': (p) => {
      const target = String(p.id);
      requireDeck(target);
      const doomed = new Set(decks.filter((d) => ancestors(d.id).includes(target)).map((d) => d.id));
      for (let i = decks.length - 1; i >= 0; i--) if (doomed.has(decks[i]!.id)) decks.splice(i, 1);
      emit('deck.deleted', { id: target });
      return null;
    },
    'v1.attribute.effective': (p) => {
      const chain = ancestors(String(p.deck_id));
      return attrs.filter((a) => a.scope_deck_id === null || chain.includes(a.scope_deck_id));
    },
    'v1.note.create': (p) => {
      const deckId = String(p.deck_id);
      requireDeck(deckId);
      const fields = (p.fields ?? {}) as Record<string, string>;
      if (!fields.front?.trim()) throw err('VALIDATION_EMPTY_FIELD', 'errors.validation.empty_front', '');
      const n: NoteSummary = {
        id: id('note'),
        deck_id: deckId,
        notetype_id: String(p.notetype_id ?? 'classic'),
        sort_field: fields.front,
        fields,
        attributes: (p.attributes ?? {}) as NoteSummary['attributes'],
        created_at: seq,
      };
      notes.push(n);
      emit('note.created', { id: n.id, deck_id: deckId });
      return { id: n.id };
    },
    'v1.note.list': (p) => notes.filter((n) => n.deck_id === p.deck_id),
    'v1.note.delete': (p) => {
      const i = notes.findIndex((n) => n.id === p.id);
      if (i < 0) throw err('NOT_FOUND_NOTE', 'errors.note.not_found', String(p.id));
      notes.splice(i, 1);
      emit('note.deleted', { id: p.id });
      return null;
    },
  };

  return {
    async invoke(command, payload) {
      const h = handlers[command];
      if (!h) throw err('NOT_IMPLEMENTED', 'errors.not_implemented', command);
      // Deep-copy in and out so callers can't mutate mock state (mirrors IPC serialisation).
      const input = structuredClone((payload ?? {}) as Record<string, unknown>);
      return structuredClone(h(input));
    },
    async listen(handler) {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  };
}
