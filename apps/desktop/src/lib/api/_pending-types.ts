/**
 * TEMPORARY — DELETE AT INTEGRATION.
 *
 * Hand-written stand-ins for the DTOs that `fc-api` will generate into
 * `src/lib/api/generated/` (specta / tauri-specta). Nothing outside
 * `src/lib/api/types.ts` may import this file directly. At integration:
 *   1. delete this file,
 *   2. point `types.ts` at `./generated`,
 *   3. fix whatever `pnpm check` reports (that is the point of generated types).
 *
 * Shapes follow docs/03-mimari.md §4, §7, §8, §11 and docs/02 §8.1.
 */

export type Uuid = string;

/** docs/03 §7 error model. `code` drives behaviour, `message_key` drives display. */
export interface ApiErrorDto {
  code: string;
  message_key: string;
  message: string;
  details?: Record<string, unknown> | null;
  retryable: boolean;
}

/** Domain event pushed on the `fc://event` channel (docs/03 §8). */
export interface FcEvent {
  type: string;
  payload: unknown;
}

export interface DeckNode {
  id: Uuid;
  parent_id: Uuid | null;
  name: string;
  children: DeckNode[];
}

export type AttributeKind =
  | 'ordered_enum'
  | 'enum'
  | 'multi_enum'
  | 'number'
  | 'text'
  | 'date'
  | 'bool';

export interface AttributeOption {
  id: Uuid;
  label: string;
  color: string | null;
  order: number;
}

/** An effective classification for a deck (inherited down the deck tree). */
export interface AttributeDef {
  id: Uuid;
  scope_deck_id: Uuid | null;
  key: string;
  name: string;
  kind: AttributeKind;
  options: AttributeOption[];
  target: 'note' | 'card';
  required: boolean;
  default_value: AttributeValue;
  position: number;
}

/** option id | option ids | number | text | ISO date (YYYY-MM-DD) | bool | unset */
export type AttributeValue = string | string[] | number | boolean | null;

export interface NoteSummary {
  id: Uuid;
  deck_id: Uuid;
  notetype_id: string;
  sort_field: string;
  fields: Record<string, string>;
  attributes: Record<Uuid, AttributeValue>;
  created_at: number;
}

export interface UiContribution {
  kind: 'menu_item';
  id: string;
  label_key: string;
  /** Screen id the frontend knows how to draw. Unknown screens are ignored. */
  screen: string;
  position: number;
}

export interface ModuleInfo {
  id: string;
  name_key: string;
  enabled: boolean;
  ui_contributions: UiContribution[];
}

/** Command / query catalogue: name → { input, output }. */
export interface CommandMap {
  'v1.modules.list': { input: Record<string, never>; output: ModuleInfo[] };
  'v1.deck.tree': { input: Record<string, never>; output: DeckNode[] };
  'v1.deck.create': { input: { name: string; parent_id: Uuid | null }; output: { id: Uuid } };
  'v1.deck.rename': { input: { id: Uuid; name: string }; output: null };
  'v1.deck.delete': { input: { id: Uuid }; output: null };
  'v1.attribute.effective': { input: { deck_id: Uuid }; output: AttributeDef[] };
  'v1.note.create': {
    input: {
      deck_id: Uuid;
      notetype_id: string;
      fields: Record<string, string>;
      attributes: Record<Uuid, AttributeValue>;
    };
    output: { id: Uuid };
  };
  'v1.note.list': { input: { deck_id: Uuid }; output: NoteSummary[] };
  'v1.note.delete': { input: { id: Uuid }; output: null };
}
