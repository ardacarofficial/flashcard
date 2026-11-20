/**
 * The slice of the local HTTP API (docs/06 §3 "Uygulanan hâli") the extension speaks. Bodies are
 * the generated `fc-api` types (`@fc/api`), never hand-written DTOs; this file only names paths,
 * the port and scopes, and re-exports the types the extension uses.
 */
export type {
  ApiError,
  AttributeKind,
  AttributeValue,
  DeckDto,
  DeckList,
  EffectiveAttributeDto,
  EffectiveAttributeList,
  EffectiveAttributes,
} from '@fc/api';
// v0.9 local-API types: see the note in this file (switches to `@fc/api` at integration).
export type {
  AddInboxItem,
  ApiClientDto,
  InboxAttributeValue,
  InboxItemDto,
  PairRequest,
  PairResponse,
  PairStatusResponse,
  PingResponse,
} from './local-api-types';

/** Default port of the desktop app's local API (setting `local_api.port`). */
export const DEFAULT_PORT = 28735;

/** `PingResponse.app` of our desktop app (anything else on the port is not us). */
export const APP_ID = 'flashcard';

/** Paths, relative to `http://127.0.0.1:<port>`. */
export const PATHS = {
  ping: '/v1/ping',
  pair: '/v1/pair',
  pairStatus: (id: string): string => `/v1/pair/${encodeURIComponent(id)}`,
  auth: '/v1/auth',
  decks: '/v1/decks',
  method: (name: string): string => `/v1/${name}`,
  inbox: '/v1/inbox',
} as const;

/** Scopes the extension asks for when pairing (the user may narrow them). */
export const SCOPES = ['inbox:write', 'decks:read', 'attributes:read'] as const;
