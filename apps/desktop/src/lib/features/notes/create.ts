/**
 * View model of the Create screen's term rows. Only shaping for display and for the
 * `notes.create_many` request: validation, duplicates and cards are decided in Rust.
 */
import type { FieldDef, ImportPreview, NoteFieldsDraft, NoteTypeDto } from '$lib/api/types';

export interface Row {
  /** Stable key for `{#each}` (rows are inserted and removed). */
  key: number;
  /** Field key → text, for the note type's editable fields. */
  fields: Record<string, string>;
  /** Markdown/`media:` snippet of an attached image (`null` = none). */
  image: string | null;
}

let seq = 0;

export function blankRow(): Row {
  return { key: ++seq, fields: {}, image: null };
}

export function blankRows(n: number): Row[] {
  return Array.from({ length: n }, blankRow);
}

/** Fields typed into a row (text kinds; an image field gets the row's image button). */
export function editableFields(nt: NoteTypeDto | null): FieldDef[] {
  if (!nt) return [];
  return nt.fields.filter((f) => (f.kind === 'text' || f.kind === 'rich_text' || f.kind === 'number') && f.key !== 'masks');
}

/** The note type's own image field, if it has one. */
export function imageField(nt: NoteTypeDto | null): FieldDef | null {
  return nt?.fields.find((f) => f.kind === 'image') ?? null;
}

export const isBlank = (r: Row) => !r.image && Object.values(r.fields).every((v) => !v.trim());

/**
 * The request rows. The image goes into the type's image field, else (Classic) it is
 * appended to the last text field as Markdown, as the note editor does.
 */
export function toDrafts(rows: Row[], nt: NoteTypeDto): NoteFieldsDraft[] {
  const fields = editableFields(nt);
  const img = imageField(nt);
  const last = fields[fields.length - 1];
  return rows.map((r) => {
    const out: Record<string, string> = {};
    for (const f of fields) out[f.key] = r.fields[f.key] ?? '';
    if (r.image) {
      if (img) out[img.key] = r.image.match(/media:[0-9a-f]{64}/)?.[0] ?? r.image;
      else if (last) out[last.key] = [out[last.key], r.image].filter((s) => s?.trim()).join('\n\n');
    }
    return { fields: out };
  });
}

/** Rows from a paste preview, columns placed as the backend suggested. */
export function rowsFromPreview(preview: ImportPreview): Row[] {
  return preview.rows.map((cells) => {
    const row = blankRow();
    preview.suggested_columns.forEach((c, i) => {
      if (c.kind === 'field' && cells[i] !== undefined) row.fields[c.key] = cells[i]!;
    });
    return row;
  });
}

/** Appends pasted rows, replacing the blank rows at the end of the list. */
export function appendRows(rows: Row[], added: Row[]): Row[] {
  let end = rows.length;
  while (end > 0 && isBlank(rows[end - 1]!)) end--;
  return [...rows.slice(0, end), ...added];
}
