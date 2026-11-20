import type { ColumnTarget } from '$lib/api/types';

/** `<select>` value of a column target. */
export function encodeColumn(c: ColumnTarget): string {
  switch (c.kind) {
    case 'field':
      return `field:${c.key}`;
    case 'attribute':
      return `attr:${c.attr_id}`;
    default:
      return c.kind;
  }
}

/** Inverse of {@link encodeColumn}. */
export function decodeColumn(v: string): ColumnTarget {
  if (v.startsWith('field:')) return { kind: 'field', key: v.slice(6) };
  if (v.startsWith('attr:')) return { kind: 'attribute', attr_id: v.slice(5) };
  if (v === 'tags') return { kind: 'tags' };
  return { kind: 'ignore' };
}

/** Separator as typed in an input: a tab is written `\t`. */
export function escapeSeparator(s: string): string {
  return s.replaceAll('\t', '\\t');
}

/** Inverse of {@link escapeSeparator}. */
export function unescapeSeparator(s: string): string {
  return s.replaceAll('\\t', '\t');
}
