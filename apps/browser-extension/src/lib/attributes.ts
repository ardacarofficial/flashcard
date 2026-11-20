/**
 * Classification fields of the "add word" popup (docs/02 §8.1): the deck's effective
 * classifications (`attributes.effective`, note target) become form fields; the user's choices
 * become the inbox item's `attributes` map. Pure: no DOM, no storage.
 */
import type { AttributeKind, AttributeValue, EffectiveAttributeDto, InboxAttributeValue } from './contract';

/** One form field. */
export type Field = {
  attrId: string;
  key: string;
  name: string;
  kind: AttributeKind;
  required: boolean;
  /** Choices, in the user's order (enum kinds only). */
  options: { id: string; label: string }[];
  /** Pre-filled value: last used for this deck, else the deck's suggestion/default. */
  initial: AttributeValue | null;
};

/** Raw value as read from a control. */
export type Raw = string | string[] | boolean | null;

/** Builds the fields. `remembered` maps attribute id → last value used (null = left empty). */
export function fieldsFor(
  list: EffectiveAttributeDto[],
  remembered: Record<string, AttributeValue | null> = {},
): Field[] {
  return list
    .filter((e) => e.def.target === 'note')
    .map((e) => {
      const d = e.def;
      const options = [...d.options].sort((a, b) => a.order - b.order).map((o) => ({ id: o.id, label: o.label }));
      const rememberedValue = Object.hasOwn(remembered, d.id) ? remembered[d.id] ?? null : undefined;
      let initial = rememberedValue !== undefined ? rememberedValue : (e.suggested_value ?? d.default_value);
      if (initial && !fits(d.kind, options, initial)) initial = null;
      return {
        attrId: d.id,
        key: d.key,
        name: d.name,
        kind: d.kind,
        required: d.required,
        options,
        initial,
      };
    });
}

function fits(kind: AttributeKind, options: { id: string }[], v: AttributeValue): boolean {
  const known = (id: string): boolean => options.some((o) => o.id === id);
  switch (kind) {
    case 'ordered_enum':
    case 'enum':
      return v.type === 'choice' && known(v.value);
    case 'multi_enum':
      return v.type === 'choices' && v.value.every(known);
    case 'number':
      return v.type === 'number';
    case 'text':
      return v.type === 'text';
    case 'date':
      return v.type === 'date';
    case 'bool':
      return v.type === 'bool';
  }
}

/** Parses a control's raw value; `null` = empty. Invalid numbers/dates come back as `undefined`. */
export function parseRaw(kind: AttributeKind, raw: Raw): AttributeValue | null | undefined {
  switch (kind) {
    case 'ordered_enum':
    case 'enum':
      return typeof raw === 'string' && raw !== '' ? { type: 'choice', value: raw } : null;
    case 'multi_enum': {
      const ids = Array.isArray(raw) ? raw.filter((s) => s !== '') : [];
      return ids.length > 0 ? { type: 'choices', value: ids } : null;
    }
    case 'number': {
      if (typeof raw !== 'string' || raw.trim() === '') return null;
      const n = Number(raw.trim().replace(',', '.'));
      return Number.isFinite(n) ? { type: 'number', value: n } : undefined;
    }
    case 'text':
      return typeof raw === 'string' && raw.trim() !== '' ? { type: 'text', value: raw.trim() } : null;
    case 'date':
      if (typeof raw !== 'string' || raw === '') return null;
      return /^\d{4}-\d{2}-\d{2}$/.test(raw) ? { type: 'date', value: raw } : undefined;
    case 'bool':
      return typeof raw === 'boolean' ? { type: 'bool', value: raw } : null;
  }
}

/** Result of reading the form. */
export type Collected = {
  /**
   * `AddInboxItem.attributes`: attribute **id** → option id(s) / plain value. Ids, not labels, so a
   * rename in the app between capture and acceptance cannot break it. Empty fields are left out.
   */
  attributes: Record<string, InboxAttributeValue>;
  /** What to remember for next time (attribute id → value, null = empty). */
  remember: Record<string, AttributeValue | null>;
  /** Names of required fields left empty. */
  missing: string[];
  /** Names of fields with an unparsable value. */
  invalid: string[];
};

/** Reads raw values (attribute id → raw) into assignments and validation results. */
export function collect(fields: Field[], raw: Record<string, Raw>): Collected {
  const out: Collected = { attributes: {}, remember: {}, missing: [], invalid: [] };
  for (const f of fields) {
    const v = parseRaw(f.kind, raw[f.attrId] ?? null);
    if (v === undefined) {
      out.invalid.push(f.name);
      continue;
    }
    out.remember[f.attrId] = v;
    if (v === null) {
      if (f.required) out.missing.push(f.name);
      continue;
    }
    out.attributes[f.attrId] = toWire(v);
  }
  return out;
}

/** `AttributeValue` → the inbox's plain form (option ids for choices). */
export function toWire(v: AttributeValue): InboxAttributeValue {
  return v.value;
}
