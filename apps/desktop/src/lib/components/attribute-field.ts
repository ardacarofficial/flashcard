import type { AttributeDef, AttributeKind, AttributeOption, AttributeValue } from '$lib/api/types';

/** Which input widget renders a classification of a given kind (docs/02 §8.1). */
export type FieldControl = 'select' | 'multiselect' | 'number' | 'text' | 'date' | 'checkbox';

const CONTROL: Record<AttributeKind, FieldControl> = {
  ordered_enum: 'select',
  enum: 'select',
  multi_enum: 'multiselect',
  number: 'number',
  text: 'text',
  date: 'date',
  bool: 'checkbox',
};

export function controlFor(kind: AttributeKind): FieldControl {
  return CONTROL[kind];
}

/**
 * Options in display order. The backend owns ordering (`order`); we only
 * respect it, never invent it.
 */
export function displayOptions(def: AttributeDef): AttributeOption[] {
  return [...def.options].sort((a, b) => a.order - b.order);
}

/** Starting value for the form: the def's default, or the kind's empty value. */
export function initialValue(def: AttributeDef): AttributeValue {
  if (def.default_value !== null && def.default_value !== undefined) return def.default_value;
  switch (controlFor(def.kind)) {
    case 'multiselect':
      return [];
    case 'checkbox':
      return false;
    default:
      return null;
  }
}

/** Convert raw input-element state back into an AttributeValue (no validation: Rust validates). */
export function fromInput(kind: AttributeKind, raw: string | string[] | boolean): AttributeValue {
  switch (controlFor(kind)) {
    case 'multiselect':
      return Array.isArray(raw) ? raw : [];
    case 'checkbox':
      return raw === true;
    case 'number': {
      if (typeof raw !== 'string' || raw.trim() === '') return null;
      const n = Number(raw);
      return Number.isFinite(n) ? n : null;
    }
    default:
      return typeof raw === 'string' && raw !== '' ? raw : null;
  }
}
