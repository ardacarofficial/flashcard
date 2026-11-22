/**
 * What a note type's fields CONTAIN (v1.3, CONTRACT §2): `term`, `surface`, `meaning`,
 * `example`, `example_translation`, `source`, `note`. The backend owns roles (`FieldDef.role`);
 * this file only reads them for display — captions, which fields Quick Add shows first, which
 * column of a pasted list goes where.
 *
 * Until a backend without roles is gone, the stable field keys of the known built-in note
 * types stand in (never translated labels: a user may call any field anything).
 */
import type { FieldDef, NoteTypeDto } from '$lib/api/types';

export const ROLES = ['term', 'surface', 'meaning', 'example', 'example_translation', 'source', 'note'] as const;
export type Role = (typeof ROLES)[number];

/** Built-in note types (`NoteTypeDto.builtin`) → field key → role. */
const BUILTIN: Record<string, Record<string, Role>> = {
  classic: { front: 'term', back: 'meaning' },
  classic_reverse: { front: 'term', back: 'meaning' },
  optional_reverse: { front: 'term', back: 'meaning' },
  typing: { front: 'term', back: 'meaning' },
  vocabulary: { word: 'term', meaning: 'meaning', example: 'example' },
  subtitle: { word: 'term', surface: 'surface', meaning: 'meaning', sentence: 'example', translation: 'example_translation', source: 'source' },
};

const isRole = (v: unknown): v is Role => typeof v === 'string' && (ROLES as readonly string[]).includes(v);

/** Role of `field` in `nt`: the backend's when it sends one, else the built-in table's. */
export function fieldRole(nt: NoteTypeDto | null, field: FieldDef): Role | null {
  // Capability check: `role` arrives with the v1.3 API (generated types may not have it yet).
  const sent = (field as FieldDef & { role?: unknown }).role;
  if (sent !== undefined) return isRole(sent) ? sent : null;
  return (nt?.builtin && BUILTIN[nt.builtin]?.[field.key]) || null;
}

/** The field holding `role` in `nt`, if any. */
export function fieldWithRole(nt: NoteTypeDto | null, role: Role): FieldDef | null {
  return nt?.fields.find((f) => fieldRole(nt, f) === role) ?? null;
}

/** Field kinds a text cell (a pasted column, a grid cell) can fill. */
export const isTextField = (f: FieldDef) => f.kind === 'text' || f.kind === 'rich_text' || f.kind === 'number';

/**
 * The text fields in the order a plain list fills them: term, meaning, example, its
 * translation, source, surface form, note; then fields without a role, in the type's order.
 * ("word · meaning · sentence" lands in the right fields of any vocabulary type.)
 */
export function fillOrder(nt: NoteTypeDto | null): FieldDef[] {
  if (!nt) return [];
  const text = nt.fields.filter((f) => isTextField(f) && f.key !== 'masks');
  const rank = (f: FieldDef) => {
    const r = fieldRole(nt, f);
    const order: Role[] = ['term', 'meaning', 'example', 'example_translation', 'source', 'surface', 'note'];
    return r ? order.indexOf(r) : order.length;
  };
  return text
    .map((f, i) => ({ f, i }))
    .sort((a, b) => rank(a.f) - rank(b.f) || a.i - b.i)
    .map((x) => x.f);
}

/**
 * Fields Quick Add shows up front: the term, the meaning, the example sentence (optional) and
 * any other required field. The rest (pronunciation, audio, image…) wait under "More".
 * A type without roles shows every field, as before.
 */
export function primaryFields(nt: NoteTypeDto | null): FieldDef[] {
  if (!nt) return [];
  const roles = nt.fields.map((f) => fieldRole(nt, f));
  if (roles.every((r) => r === null)) return nt.fields;
  return nt.fields.filter((f, i) => f.required || ['term', 'meaning', 'example'].includes(roles[i] ?? ''));
}
