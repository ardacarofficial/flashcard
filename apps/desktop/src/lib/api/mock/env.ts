/**
 * Shared state and helpers of the dev mock backend. Feature handlers
 * (`features/<feature>/mock.ts`) read and write it; the transport only dispatches.
 * Not a source of truth for behaviour: the real rules live in fc-app.
 */
import type { MethodName, RequestOf, ResponseOf } from '../generated/methods';
import { DEMO_COUNTS, seedDemo } from './seed';
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

export function err(code: ErrorCode, message_key: string, message: string): ApiError {
  return { code, message_key, message, details: {}, retryable: false };
}

const CEFR = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'];

export function menu(id: string, label_key: string, route: string, order: number): UiContribution {
  return { slot: 'main_menu', id, label_key, icon: null, route, method: null, order };
}

export function settingsPage(id: string, label_key: string, route: string, order: number): UiContribution {
  return { slot: 'settings_page', id, label_key, icon: null, route, method: null, order };
}

export function module(id: string, ui: UiContribution[]): ModuleDto {
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

export type Handlers = { [M in MethodName]?: (req: RequestOf<M>) => ResponseOf<M> };

/** What a feature contributes to the mock backend (`features/<feature>/mock.ts`). */
export interface MockFeature {
  /** Modules this feature adds to `modules.list`. */
  modules?: ModuleDto[];
  handlers: (env: MockEnv) => Handlers;
}

/**
 * Mock-mode scenario from the URL, for screenshot states that need a different start
 * (`?mock=fresh` empty collection → first-run guide, `?mock=update` an update is out,
 * `?mock=locked` the app starts locked, PIN 1234). Only `pnpm dev:mock` reads it.
 */
export type MockScenario = 'fresh' | 'update' | 'locked';
export function mockScenario(): MockScenario | null {
  if (import.meta.env.MODE !== 'mock') return null;
  try {
    const v = new URLSearchParams(globalThis.location?.search ?? '').get('mock');
    return v === 'fresh' || v === 'update' || v === 'locked' ? v : null;
  } catch {
    return null;
  }
}

export function createMockEnv() {
let seq = 0;
let eventSeq = 0;
/**
 * Mock clock in **milliseconds since the Unix epoch**, like `fc_core::Timestamp` (the DTOs'
 * `*_at` fields). It starts a day before the env is created and ticks one second per call, so
 * records stay ordered and look recent (a bare counter showed up as 01.01.1970).
 */
const clockStart = Date.now() - 86_400_000;
let tick = 0;
const now = () => clockStart + ++tick * 1000;
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
// Demo data only in `pnpm dev:mock` (vite --mode mock); tests start from an empty collection.
if (mockScenario() === 'fresh') {
  // A brand-new collection: no decks at all (the first-run guide shows).
  decks.length = 0;
  attrs.length = 0;
} else if (import.meta.env.MODE === 'mock') seedDemo({ now, id, decks, attrs, notes, addDeck }, english.id);
/** Flag bits per card id (`cards.set_flag` writes, card views read). */
const cardFlags = new Map<string, number>();

const tree = (parent: string | null): DeckTreeNode[] =>
  decks
    .filter((d) => d.parent_id === parent)
    .sort((a, b) => a.position - b.position || a.name.localeCompare(b.name))
    .map((deck) => ({
      deck: { ...deck, path: pathOf(deck) },
      counts: (import.meta.env.MODE === 'mock' && DEMO_COUNTS[deck.name]) || { new: 0, learning: 0, due: 0, total: 0 },
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
  return {
    listeners,
    now,
    id,
    emit,
    decks,
    attrs,
    notes,
    cardFlags,
    pathOf,
    addDeck,
    tree,
    ancestors,
    requireDeck,
    requireName,
    sameName,
  };
}

export type MockEnv = ReturnType<typeof createMockEnv>;
