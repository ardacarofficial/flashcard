import {
  API_VERSION,
  type MethodName,
  type RequestOf,
  type ResponseOf,
} from './generated/methods';
import type {
  ApiError as ApiErrorDto,
  AttributeAssignment,
  DeckId,
  DomainEvent,
  ErrorCode,
  EventEnvelope,
  NoteId,
  NoteTypeId,
} from './types';
import type { Transport, Unsubscribe } from './transport';

/** Built-in Classic note type (`fc_core::notetype::builtin::CLASSIC_ID`, seeded by the app). */
export const CLASSIC_NOTETYPE_ID: NoteTypeId = '01900000-0000-7000-8000-000000000001';

/** Qualified wire name of a method: `decks.create` → `v1.decks.create`. */
export function wireName(method: MethodName): string {
  return `${API_VERSION}.${method}`;
}

/** Error thrown by every api call. UI branches on `code`, displays `messageKey`. */
export class ApiError extends Error {
  readonly code: ErrorCode;
  readonly messageKey: string;
  readonly details: Record<string, unknown>;
  readonly retryable: boolean;

  constructor(dto: ApiErrorDto) {
    super(dto.message || dto.code);
    this.name = 'ApiError';
    this.code = dto.code;
    this.messageKey = dto.message_key;
    this.details = dto.details ?? {};
    this.retryable = dto.retryable;
  }
}

function isErrorDto(v: unknown): v is ApiErrorDto {
  return (
    typeof v === 'object' &&
    v !== null &&
    typeof (v as { code?: unknown }).code === 'string' &&
    typeof (v as { message_key?: unknown }).message_key === 'string'
  );
}

/** Normalise anything a transport may throw into an ApiError. */
export function toApiError(e: unknown): ApiError {
  if (e instanceof ApiError) return e;
  if (isErrorDto(e)) return new ApiError(e);
  if (typeof e === 'string') {
    try {
      const parsed: unknown = JSON.parse(e);
      if (isErrorDto(parsed)) return new ApiError(parsed);
    } catch {
      /* not JSON */
    }
  }
  return new ApiError({
    code: 'INTERNAL',
    message_key: 'errors.internal.unexpected',
    message: e instanceof Error ? e.message : String(e),
    details: {},
    retryable: false,
  });
}

/** Discriminant of a domain event (`deck_created`, `notes_deleted`, …). */
export type EventType = DomainEvent['type'];
export type EventHandler = (event: EventEnvelope) => void;

export function createApiClient(transport: Transport) {
  /** Calls `method` with typed params; resolves to its typed response or throws ApiError. */
  async function call<M extends MethodName>(method: M, params: RequestOf<M>): Promise<ResponseOf<M>> {
    try {
      return (await transport.invoke(wireName(method), params)) as ResponseOf<M>;
    } catch (e) {
      throw toApiError(e);
    }
  }

  /**
   * Subscribe to domain events whose type starts with one of `prefixes` (`''` = all; e.g.
   * `'deck_'` matches `deck_created`, `'note'` matches `note_created` and `notes_deleted`).
   * Returns an unsubscribe function.
   */
  function on(prefixes: string | readonly string[], handler: EventHandler): Promise<Unsubscribe> {
    const list = typeof prefixes === 'string' ? [prefixes] : prefixes;
    return transport.listen((env) => {
      if (list.some((p) => env.event.type.startsWith(p))) handler(env);
    });
  }

  return {
    call,
    on,
    modules: {
      list: () => call('modules.list', {}),
    },
    deck: {
      tree: () => call('decks.tree', {}),
      /**
       * Creates the deck `name` under `parentId` (`null` = top level). The backend builds the
       * full path; a name containing `::` creates the missing levels.
       */
      create: (name: string, parentId: DeckId | null = null) =>
        call('decks.create', { path: name, parent_id: parentId }),
      rename: (id: DeckId, name: string) => call('decks.rename', { id, name }),
      remove: (id: DeckId) => call('decks.delete', { id }),
    },
    attribute: {
      /** Note-level classifications that apply to a deck (inherited down the tree). */
      effective: (deckId: DeckId) =>
        call('attributes.effective', { deck_id: deckId, target: 'note' }),
    },
    note: {
      createClassic: (deckId: DeckId, front: string, back: string, attributes: AttributeAssignment[]) =>
        call('notes.create', {
          notetype_id: CLASSIC_NOTETYPE_ID,
          deck_id: deckId,
          fields: { front, back },
          tags: [],
          attributes,
        }),
      list: (deckId: DeckId) =>
        call('notes.list', {
          deck_id: deckId,
          include_subdecks: false,
          page: { cursor: null, limit: null },
        }),
      remove: (id: NoteId) => call('notes.delete', { ids: [id] }),
    },
    oplog: {
      undo: () => call('oplog.undo', {}),
      redo: () => call('oplog.redo', {}),
    },
  };
}

export type ApiClient = ReturnType<typeof createApiClient>;
