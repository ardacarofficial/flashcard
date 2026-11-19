import type { ApiErrorDto, AttributeValue, CommandMap, FcEvent, Uuid } from './types';
import type { Transport, Unsubscribe } from './transport';

export type CommandName = keyof CommandMap;
export type Input<K extends CommandName> = CommandMap[K]['input'];
export type Output<K extends CommandName> = CommandMap[K]['output'];

/** Error thrown by every api call. UI branches on `code`, displays `messageKey`. */
export class ApiError extends Error {
  readonly code: string;
  readonly messageKey: string;
  readonly details: Record<string, unknown> | null;
  readonly retryable: boolean;

  constructor(dto: ApiErrorDto) {
    super(dto.message || dto.code);
    this.name = 'ApiError';
    this.code = dto.code;
    this.messageKey = dto.message_key;
    this.details = dto.details ?? null;
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
    message_key: 'errors.internal',
    message: e instanceof Error ? e.message : String(e),
    details: null,
    retryable: false,
  });
}

export type EventHandler = (event: FcEvent) => void;

export function createApiClient(transport: Transport) {
  async function call<K extends CommandName>(command: K, payload: Input<K>): Promise<Output<K>> {
    try {
      return (await transport.invoke(command, payload)) as Output<K>;
    } catch (e) {
      throw toApiError(e);
    }
  }

  /**
   * Subscribe to domain events. `prefix` filters by event type
   * (e.g. "deck." matches "deck.created"). Returns an unsubscribe function.
   */
  function on(prefix: string, handler: EventHandler): Promise<Unsubscribe> {
    return transport.listen((ev) => {
      if (prefix === '' || ev.type.startsWith(prefix)) handler(ev);
    });
  }

  const none = {} as Record<string, never>;

  return {
    call,
    on,
    modules: {
      list: () => call('v1.modules.list', none),
    },
    deck: {
      tree: () => call('v1.deck.tree', none),
      create: (name: string, parentId: Uuid | null) =>
        call('v1.deck.create', { name, parent_id: parentId }),
      rename: (id: Uuid, name: string) => call('v1.deck.rename', { id, name }),
      remove: (id: Uuid) => call('v1.deck.delete', { id }),
    },
    attribute: {
      effective: (deckId: Uuid) => call('v1.attribute.effective', { deck_id: deckId }),
    },
    note: {
      createClassic: (
        deckId: Uuid,
        front: string,
        back: string,
        attributes: Record<Uuid, AttributeValue>,
      ) =>
        call('v1.note.create', {
          deck_id: deckId,
          notetype_id: 'classic',
          fields: { front, back },
          attributes,
        }),
      list: (deckId: Uuid) => call('v1.note.list', { deck_id: deckId }),
      remove: (id: Uuid) => call('v1.note.delete', { id }),
    },
  };
}

export type ApiClient = ReturnType<typeof createApiClient>;
