/**
 * The note type editor's "what does this field contain?" (`FieldDef.role`) and "what does this
 * card ask for?" (`CardTemplate.answer_field`) edits (v1.3, CONTRACT §2). Draft shaping only:
 * the backend validates (one field per role, an existing answer field) and stores.
 *
 * An unset role / answer field is left out of the object (as the backend sends it), so
 * choosing something and then "nothing" again leaves the draft unchanged.
 */
import type { CardTemplate, FieldDef, FieldRole, NoteTypeInput } from '$lib/api/types';

/** In the order the picker lists them. */
export const FIELD_ROLES: FieldRole[] = ['term', 'surface', 'meaning', 'example', 'example_translation', 'source', 'note'];

/** Engines that decide their expected answer themselves (no answer field to pick). */
const OWN_ANSWER = new Set(['cloze', 'multiple_choice', 'image_occlusion', 'typing']);

/** Fields that can hold a role: text kinds (and any field that already has one). */
export const takesRole = (f: FieldDef) => f.kind === 'text' || f.kind === 'rich_text' || f.kind === 'number' || !!f.role;

/** Templates whose card asks for one field's text (self-graded / typed-answer comparison). */
export const asksForField = (t: CardTemplate) => !OWN_ANSWER.has(t.card_type);

/**
 * Field `i` gets `role` (`null` clears it). A role belongs to one field: another field holding
 * it loses it.
 */
export function withRole(fields: FieldDef[], i: number, role: FieldRole | null): FieldDef[] {
  return fields.map((f, j) => {
    if (j !== i && (role === null || f.role !== role)) return f;
    const { role: _old, ...rest } = f;
    return j === i && role ? { ...rest, role } : rest;
  });
}

/** Template `i` asks for field `key` (`null`: its whole answer side). */
export function withAnswerField(templates: CardTemplate[], i: number, key: string | null): CardTemplate[] {
  return templates.map((t, j) => {
    if (j !== i) return t;
    const { answer_field: _old, ...rest } = t;
    return key ? { ...rest, answer_field: key } : rest;
  });
}

/**
 * The definition without field `i`. Templates that asked for it ask for their whole answer
 * side again (as the backend does on save).
 */
export function withoutField(def: NoteTypeInput, i: number): Pick<NoteTypeInput, 'fields' | 'templates'> {
  const gone = def.fields[i]?.key;
  return {
    fields: def.fields.filter((_, j) => j !== i),
    templates: gone ? def.templates.map((t, j) => (t.answer_field === gone ? withAnswerField(def.templates, j, null)[j]! : t)) : def.templates,
  };
}
