import type { AttributeDef, AttributeKind, AttributeOption, AttributeValue } from '$lib/api/types';

/** Which input widget renders a classification of a given kind (docs/02 §8.1). */
export type FieldControl = 'select' | 'multiselect' | 'number' | 'text' | 'date' | 'tristate';

const CONTROL: Record<AttributeKind, FieldControl> = {
  ordered_enum: 'select',
  enum: 'select',
  multi_enum: 'multiselect',
  number: 'number',
  text: 'text',
  date: 'date',
  bool: 'tristate',
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

/**
 * Starting value for the form: the def's default if the backend sent one,
 * otherwise `null` ("untouched"). Defaults are applied by Rust, never here.
 */
export function initialValue(def: AttributeDef): AttributeValue {
  return def.default_value ?? null;
}

/** Convert raw input-element state back into an AttributeValue (no validation: Rust validates). */
export function fromInput(kind: AttributeKind, raw: string | string[]): AttributeValue {
  switch (controlFor(kind)) {
    case 'multiselect':
      return Array.isArray(raw) && raw.length > 0 ? raw : null;
    case 'tristate':
      return raw === 'true' ? true : raw === 'false' ? false : null;
    case 'number': {
      if (typeof raw !== 'string' || raw.trim() === '') return null;
      const n = Number(raw);
      return Number.isFinite(n) ? n : null;
    }
    default:
      return typeof raw === 'string' && raw !== '' ? raw : null;
  }
}
