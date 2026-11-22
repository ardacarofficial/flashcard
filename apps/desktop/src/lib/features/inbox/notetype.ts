/**
 * Which note type an inbox proposal becomes (display + the accept request). Mirrors the
 * backend's rule (fc-app `inbox::default_notetype`, v1.3): the item's own note type; else an
 * item that came with a captured sentence (and no field values of its own) becomes the
 * built-in Subtitle card, so the sentence is kept; else Classic. The row says which, and an
 * accepted edit names the same type, so editing the word never turns it into a Classic card.
 */
import { CLASSIC_NOTETYPE_ID } from '$lib/api';
import type { AddInboxItem, NoteTypeDto } from '$lib/api/types';
import { fieldWithRole } from '../notes/roles';

export interface InboxType {
  /** The note type (`null` when it is not in the loaded list). */
  type: NoteTypeDto | null;
  /** Chosen because the item came with a sentence (the UI says so). */
  bySentence: boolean;
}

const hasSentence = (item: AddInboxItem) => !!item.context?.sentence?.trim();

export function inboxNoteType(item: AddInboxItem, types: NoteTypeDto[]): InboxType {
  if (item.notetype_id) return { type: types.find((n) => n.id === item.notetype_id) ?? null, bySentence: false };
  if (hasSentence(item) && Object.keys(item.fields ?? {}).length === 0) {
    const subtitle = types.find((n) => n.builtin === 'subtitle');
    if (subtitle) return { type: subtitle, bySentence: true };
  }
  return { type: types.find((n) => n.id === CLASSIC_NOTETYPE_ID) ?? null, bySentence: false };
}

/**
 * Field keys the front / back shorthand fills in `nt` (as the backend maps it): the `term` and
 * `meaning` fields; a type without both: its first two fields; unknown type: `front` / `back`.
 */
export function shorthandKeys(nt: NoteTypeDto | null): [string, string] {
  if (!nt) return ['front', 'back'];
  const term = fieldWithRole(nt, 'term');
  const meaning = fieldWithRole(nt, 'meaning');
  if (term && meaning) return [term.key, meaning.key];
  return [nt.fields[0]?.key ?? 'front', nt.fields[1]?.key ?? 'back'];
}
