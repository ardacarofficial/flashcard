/**
 * TEMPORARY mirror of the v0.9 local-API DTOs that track I (`feat/v0.9-api`) generates into
 * `@fc/api` (docs/06 §3 "Uygulanan hâli"). Names and shapes follow that contract exactly.
 * At integration this file is deleted and `contract.ts` re-exports these names from `@fc/api`
 * (one-line change); `pnpm check` then proves the extension matches the generated types.
 */
import type { DeckId } from '@fc/api';

/** `GET /v1/ping`. */
export type PingResponse = { app: string; api_version: string; version: string };

/** `POST /v1/pair` body. */
export type PairRequest = { client_name: string; client_kind: 'browser_extension' | 'app' | 'script'; scopes: string[] };

/** `POST /v1/pair` → 201. */
export type PairResponse = { request_id: string; status: 'pending'; expires_at: number };

/** `GET /v1/pair/{request_id}`. */
export type PairStatusResponse = {
  status: 'pending' | 'approved' | 'denied' | 'expired';
  client_id?: string | null;
  token?: string | null;
  scopes: string[];
};

/** Value of one classification in `AddInboxItem.attributes`. */
export type InboxAttributeValue = string | string[] | number | boolean;

/** `POST /v1/inbox` body (`inbox.add`). */
export type AddInboxItem = {
  front?: string | null;
  back?: string | null;
  fields?: Record<string, string>;
  deck_id?: DeckId | null;
  notetype_id?: string | null;
  tags?: string[];
  attributes?: Record<string, InboxAttributeValue>;
  context?: { url?: string | null; title?: string | null; sentence?: string | null } | null;
  /** Client-chosen id (1–100 chars); a resend with the same ref returns the existing item (200). */
  client_ref?: string | null;
};

/** `POST /v1/inbox` → 201. */
export type InboxItemDto = {
  id: string;
  status: string;
  item: AddInboxItem;
  created_at: number;
  updated_at: number;
};

/** `GET /v1/auth`. */
export type ApiClientDto = { id: string; name: string; kind: string; scopes: string[]; status: string };
