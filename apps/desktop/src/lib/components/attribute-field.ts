import type {
  AttributeAssignment,
  AttributeDefDto,
  AttributeKind,
  AttributeOption,
  AttributeValue,
} from '$lib/api/types';

/** Which input widget renders a classification of a given kind (docs/02 §8.1). */
export type FieldControl = 'select' | 'multiselect' | 'number' | 'text' | 'date' | 'tristate';

/** Form state of one field: a typed value, or `null` = not set / untouched. */
export type FieldValue = AttributeValue | null;

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
export function displayOptions(def: AttributeDefDto): AttributeOption[] {
  return [...def.options].sort((a, b) => a.order - b.order);
}

/**
 * Starting value for the form: the def's default if the backend sent one,
 * otherwise `null` ("untouched"). Defaults are applied by Rust, never here.
 */
export function initialValue(def: AttributeDefDto): FieldValue {
  return def.default_value ?? null;
}

/** Convert raw input-element state into a typed value (no validation: Rust validates). */
export function fromInput(kind: AttributeKind, raw: string | string[]): FieldValue {
  const text = typeof raw === 'string' ? raw : '';
  switch (controlFor(kind)) {
    case 'multiselect':
      return Array.isArray(raw) && raw.length > 0 ? { type: 'choices', value: raw } : null;
    case 'tristate':
      return text === 'true'
        ? { type: 'bool', value: true }
        : text === 'false'
          ? { type: 'bool', value: false }
          : null;
    case 'number': {
      if (text.trim() === '') return null;
      const n = Number(text);
      return Number.isFinite(n) ? { type: 'number', value: n } : null;
    }
    case 'select':
      return text !== '' ? { type: 'choice', value: text } : null;
    case 'date':
      return text !== '' ? { type: 'date', value: text } : null;
    case 'text':
      return text !== '' ? { type: 'text', value: text } : null;
  }
}

/** The single-valued input state of a value (`''` = unset). */
export function toInput(value: FieldValue): string {
  if (value === null) return '';
  switch (value.type) {
    case 'choice':
    case 'text':
    case 'date':
      return value.value;
    case 'number':
      return String(value.value);
    case 'bool':
      return value.value ? 'true' : 'false';
    case 'choices':
      return '';
  }
}

/** Selected option ids of a multi-choice value. */
export function selectedChoices(value: FieldValue): string[] {
  return value?.type === 'choices' ? value.value : [];
}

/**
 * Form state → `CreateNote.attributes`. Unset fields are left out, so untouched
 * classifications are simply not sent (Rust applies defaults / required rules).
 */
export function toAssignments(values: Record<string, FieldValue>): AttributeAssignment[] {
  return Object.entries(values)
    .filter((e): e is [string, AttributeValue] => e[1] !== null)
    .map(([attr_id, value]) => ({ attr_id, value }));
}
